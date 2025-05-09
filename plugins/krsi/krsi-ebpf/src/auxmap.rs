use core::ptr::null_mut;

use aya_ebpf::{
    cty::c_uchar,
    helpers::{
        bpf_get_current_pid_tgid, bpf_ktime_get_boot_ns, bpf_probe_read_kernel_str_bytes,
        bpf_probe_read_user_str_bytes,
    },
    programs::FExitContext,
};
use aya_log_ebpf::info;
use krsi_common::{EventHeader, EventType};
use krsi_ebpf_core::{
    read_field, Filename, Path, Sock, Sockaddr, SockaddrIn, SockaddrIn6, SockaddrUn, Socket, Wrap,
};
use zerocopy::{FromBytes, Immutable, IntoBytes};

use crate::{defs, scap, shared_state, sockets, FileDescriptor};

/// Parameter maximum length. Since [MAX_EVENT_LEN](crate::MAX_EVENT_LEN) must be a power of 2, this
/// can be used as a mask to check that the accesses to the auxiliary map are always in bound and
/// please, in this way, the verifier.
const MAX_PARAM_VALUE_LEN: usize = crate::MAX_EVENT_LEN - 1;

/// Auxiliary map length. It must be able to contain an event of
/// [MAX_EVENT_LEN](crate::MAX_EVENT_LEN), but it is set to its double in order to please the
/// verifier.
const AUXILIARY_MAP_LEN: usize = crate::MAX_EVENT_LEN * 2;

pub struct AuxiliaryMapWriter<'a> {
    header: &'a mut EventHeader,
    lengths: &'a mut [u8],
    values: &'a mut [u8],
}

#[derive(Debug)]
pub struct NoBufSpace;

fn reserve_fixed_size_space<'a, 'b: 'a>(
    buf: &'a mut &'b mut [u8],
    value_size: usize,
) -> Result<&'a mut [u8], NoBufSpace> {
    if value_size > MAX_PARAM_VALUE_LEN || value_size > buf.len() {
        return Err(NoBufSpace);
    }

    let old_buf = core::mem::take(buf);
    let (head, tail) = old_buf.split_at_mut(value_size);
    *buf = tail;
    Ok(head)
}

fn forget<'a, 'b: 'a>(buf: &'a mut &'b mut [u8], size: usize) {
    let _ = reserve_fixed_size_space(buf, size);
}

impl AuxiliaryMapWriter<'_> {
    fn cap_max_param_value_len(&mut self, len: usize) -> usize {
        let len = len & MAX_PARAM_VALUE_LEN;
        len.min(self.values.len())
    }

    fn write<'a, 'b: 'a, T: IntoBytes + Immutable>(
        buf: &'a mut &'b mut [u8],
        value: T,
    ) -> Result<(), NoBufSpace>
    where
        T: Sized,
    {
        let reserved_space = reserve_fixed_size_space(buf, size_of::<T>())?;
        reserved_space.copy_from_slice(value.as_bytes());
        Ok(())
    }

    fn write_value<T: IntoBytes + Immutable>(&mut self, value: T) -> Result<(), NoBufSpace> {
        // TODO(ekoops): handle error
        Self::write(&mut self.values, value)
    }

    fn write_len(&mut self, len: u16) -> Result<(), NoBufSpace> {
        // TODO(ekoops): handle error
        Self::write(&mut self.lengths, len)
    }

    /// Try to push the char buffer pointed by `buf` into the underlying buffer. The maximum length
    /// of the char buffer can be at most `max_len_to_read`. In case of success, returns the number
    /// of written bytes. The written buffer always includes the `\0` character (even if it points
    /// to an empty C string), and this is accounted in the returned number of written bytes: this
    /// means that in case of success, a strictly positive integer is returned. `is_kern_mem` allows
    /// to specify if the `charbuf` points to kernel or userspace memory.
    fn write_char_buffer(
        &mut self,
        buf: *const u8,
        max_len_to_read: usize,
        is_kern_mem: bool,
    ) -> Result<u16, i64> {
        let max_len_to_read = self.cap_max_param_value_len(max_len_to_read);
        let dest = &mut self.values[..max_len_to_read];
        let written_str = if is_kern_mem {
            unsafe { bpf_probe_read_kernel_str_bytes(buf, dest) }?
        } else {
            unsafe { bpf_probe_read_user_str_bytes(buf, dest) }?
        };
        let written_bytes = written_str.len() + 1; // + 1 accounts for `\0`
        forget(&mut self.values, written_bytes);
        Ok(written_bytes as u16)
    }

    fn write_path(&mut self, path: &Path, max_len_to_read: usize) -> Result<u16, i64> {
        let max_len_to_read = self.cap_max_param_value_len(max_len_to_read);
        let written_bytes = unsafe { path.read_into(&mut self.values, max_len_to_read as u32) }?;
        if written_bytes == 0 {
            // Push '\0' (empty string) and returns 1 as number of written bytes.
            let _ = self.write_value(0_u8);
            return Ok(1);
        }
        forget(&mut self.values, written_bytes);
        Ok(written_bytes as u16)
    }

    fn write_inet_sockaddr(&mut self, sockaddr: &SockaddrIn, is_kern_sockaddr: bool) -> usize {
        let addr = sockaddr.sin_addr();
        let ipv4_addr = read_field!(addr => s_addr, is_kern_sockaddr).unwrap_or(0);
        let port = read_field!(sockaddr => sin_port, is_kern_sockaddr).unwrap_or(0);
        let _ = self.write_value(scap::encode_socket_family(defs::AF_INET));
        let _ = self.write_value(ipv4_addr);
        let _ = self.write_value(u16::from_be(port));
        defs::FAMILY_SIZE + defs::IPV4_SIZE + defs::PORT_SIZE
    }

    fn write_inet6_sockaddr(&mut self, sockaddr: &SockaddrIn6, is_kern_sockaddr: bool) -> usize {
        let addr = sockaddr.sin6_addr();
        let ipv6_addr = read_field!(addr => in6_u, is_kern_sockaddr).unwrap_or([0, 0, 0, 0]);
        let port = read_field!(sockaddr => sin6_port, is_kern_sockaddr).unwrap_or(0);
        let _ = self.write_value(scap::encode_socket_family(defs::AF_INET6));
        let _ = self.write_value(ipv6_addr);
        let _ = self.write_value(u16::from_be(port));
        defs::FAMILY_SIZE + defs::IPV6_SIZE + defs::PORT_SIZE
    }

    fn write_unix_sockaddr(&mut self, sockaddr: &SockaddrUn, is_kern_sockaddr: bool) -> usize {
        let mut path: [c_uchar; defs::UNIX_PATH_MAX] = [0; defs::UNIX_PATH_MAX];
        let _ = sockets::sockaddr_un_path_into(&sockaddr, is_kern_sockaddr, &mut path);
        let _ = self.write_value(scap::encode_socket_family(defs::AF_UNIX));
        let written_bytes = self.write_sockaddr_path(&mut path).unwrap_or(0);
        defs::FAMILY_SIZE + written_bytes as usize
    }

    fn write_sockaddr_path(&mut self, path: &[c_uchar; defs::UNIX_PATH_MAX]) -> Result<u16, i64> {
        // Notice an exception in `sun_path` (https://man7.org/linux/man-pages/man7/unix.7.html):
        // an `abstract socket address` is distinguished (from a pathname socket) by the fact that
        // sun_path[0] is a null byte (`\0`). So in this case, we need to skip the initial `\0`.
        //
        // Warning: if you try to extract the path slice in a separate statement as follows, the
        // verifier will complain (maybe because it would lose information about slice length):
        // let path_ref = if path[0] == 0 {&path[1..]} else {&path[..]};
        if path[0] == 0 {
            let path_ref = &path[1..];
            self.write_char_buffer(path_ref.as_ptr().cast(), path.len(), true)
        } else {
            let path_ref = &path[..];
            self.write_char_buffer(path_ref.as_ptr().cast(), path.len(), true)
        }
    }

    pub fn skip(&mut self, len: u16) {
        forget(&mut self.values, len as usize);
        forget(&mut self.lengths, size_of::<u16>());
    }

    pub fn push<T: IntoBytes + Immutable>(&mut self, value: T) {
        let _ = self.write_value(value);
        let _ = self.write_len(size_of::<T>() as u16);
    }
    pub fn push_empty(&mut self) {
        let _ = self.write_len(0);
    }

    /// This helper stores the charbuf pointed by `buf` into the auxmap. We read until we find a
    /// `\0`, if the charbuf length is greater than `max_len_to_read`, we read up to
    /// `max_len_to_read-1` bytes and add the `\0`. `is_kern_mem` allows to specify if the `buf`
    /// points to kernel or userspace memory. In case of error, the auxmap is left untouched.
    pub fn push_char_buffer(
        &mut self,
        buf: *const u8,
        max_len_to_read: u16,
        is_kern_mem: bool,
    ) -> Result<u16, i64> {
        let mut buf_len = 0_u16;
        if !buf.is_null() {
            buf_len = self.write_char_buffer(buf, max_len_to_read as usize, is_kern_mem)?;
        }
        let _ = self.write_len(buf_len);
        Ok(buf_len)
    }

    /// This helper stores the path pointed by `path` into the auxmap. We read until we find a `\0`,
    /// if the path length is greater than `max_len_to_read`, we read up to `max_len_to_read-1`
    /// bytes and add the `\0`.
    pub fn push_path(&mut self, path: &Path, max_len_to_read: u16) -> Result<u16, i64> {
        let mut path_len = 0_u16;
        if !path.is_null() {
            path_len = self.write_path(&path, max_len_to_read as usize)?;
        }
        let _ = self.write_len(path_len);
        Ok(path_len)
    }

    pub fn push_filename(&mut self, filename: &Filename, max_len_to_read: u16, is_kern_mem: bool) {
        if let Err(_) = filename
            .name()
            .and_then(|name| self.push_char_buffer(name.cast(), max_len_to_read, is_kern_mem))
        {
            self.push_empty();
        }
    }

    pub fn push_file_descriptor(&mut self, file_descriptor: FileDescriptor) {
        match file_descriptor.try_into() {
            Ok(FileDescriptor::Fd(fd)) => {
                self.push(fd as i64);
                self.push_empty();
            }
            Ok(FileDescriptor::FileIndex(file_index)) => {
                self.push_empty();
                self.push(file_index);
            }
        }
    }

    pub fn push_sockaddr(&mut self, sockaddr: &Sockaddr, is_kern_sockaddr: bool) {
        let sa_family = read_field!(sockaddr => sa_family, is_kern_sockaddr);
        let Ok(sa_family) = sa_family else {
            self.push_empty();
            return;
        };

        let final_parameter_len = match sa_family {
            defs::AF_INET => self.write_inet_sockaddr(&sockaddr.as_sockaddr_in(), is_kern_sockaddr),
            defs::AF_INET6 => {
                self.write_inet6_sockaddr(&sockaddr.as_sockaddr_in6(), is_kern_sockaddr)
            }
            defs::AF_UNIX => self.write_unix_sockaddr(&sockaddr.as_sockaddr_un(), is_kern_sockaddr),
            _ => 0,
        } as u16;

        let _ = self.write_len(final_parameter_len);
    }

    /// Store the socktuple obtained by extracting information from the provided socket and the
    /// provided sockaddr. Returns the stored lengths.
    pub fn push_sock_tuple(
        &mut self,
        ctx: &FExitContext,
        sock: &Socket,
        is_outbound: bool,
        sockaddr: &Sockaddr,
        is_kern_sockaddr: bool,
    ) -> u16 {
        if sock.is_null() {
            self.push_empty();
            return 0;
        }

        let Ok(sk) = sock.sk() else {
            self.push_empty();
            return 0;
        };

        let Ok(sk_family) = sk.__sk_common().skc_family() else {
            self.push_empty();
            return 0;
        };

        let final_parameter_len = match sk_family {
            defs::AF_INET => {
                self.write_inet_sock_tuple(ctx, &sk, is_outbound, sockaddr, is_kern_sockaddr)
            }
            defs::AF_INET6 => {
                self.write_inet6_sock_tuple(ctx, &sk, is_outbound, sockaddr, is_kern_sockaddr)
            }
            defs::AF_UNIX => {
                self.write_unix_sock_tuple(ctx, &sk, is_outbound, sockaddr, is_kern_sockaddr)
            }
            _ => 0,
        } as u16;

        let _ = self.write_len(final_parameter_len);
        final_parameter_len
    }

    fn write_inet_sock_tuple(
        &mut self,
        ctx: &FExitContext,
        sk: &Sock,
        is_outbound: bool,
        sockaddr: &Sockaddr,
        is_kern_sockaddr: bool,
    ) -> usize {
        let inet_sk = sk.as_inet_sock();
        let ipv4_local = inet_sk.inet_saddr().unwrap_or(0);
        let port_local = inet_sk.inet_sport().unwrap_or(0);
        let mut ipv4_remote = sk.__sk_common().skc_daddr().unwrap_or(0);
        let mut port_remote = sk.__sk_common().skc_dport().unwrap_or(0);

        // Kernel doesn't always fill sk->__sk_common in sendto and sendmsg syscalls (as in the case
        // of a UDP connection). We fall back to the address from userspace when the kernel-provided
        // address is NULL.
        if port_remote == 0 && !sockaddr.is_null() {
            let sockaddr = sockaddr.as_sockaddr_in();
            if is_kern_sockaddr {
                ipv4_remote = sockaddr.sin_addr().s_addr().unwrap_or(0);
                port_remote = sockaddr.sin_port().unwrap_or(0);
            } else {
                ipv4_remote = sockaddr.sin_addr().s_addr_user().unwrap_or(0);
                port_remote = sockaddr.sin_port_user().unwrap_or(0);
            }
        }

        // Pack the tuple info: (sock_family, local_ipv4, local_port, remote_ipv4, remote_port)
        match reserve_fixed_size_space(&mut self.values, size_of::<u8>()) {
            Ok(reserved_space) => {
                info!(ctx, "Reserved space content before: {}", reserved_space[0]);
                info!(
                    ctx,
                    "Writing : {}",
                    scap::encode_socket_family(defs::AF_INET).as_bytes()[0]
                );
                reserved_space
                    .copy_from_slice(scap::encode_socket_family(defs::AF_INET).as_bytes());
            }
            Err(_) => {}
        }
        // reserved_space.copy_from_slice(value.as_bytes());
        // if let Err(_) = self.write_value(scap::encode_socket_family(defs::AF_INET)) {
        //     info!(ctx, "error writing AF_INET");
        // }

        if is_outbound {
            if let Err(_) = self.write_value(ipv4_local) {
                info!(ctx, "outbound: error writing ipv4_local");
            }
            if let Err(_) = self.write_value(u16::from_be(port_local)) {
                info!(ctx, "outbound: error writing port_local");
            }
            if let Err(_) = self.write_value(ipv4_remote) {
                info!(ctx, "outbound: error writing ipv4_remote");
            }
            if let Err(_) = self.write_value(u16::from_be(port_remote)) {
                info!(ctx, "outbound: error writing port_remote");
            }
        } else {
            if let Err(_) = self.write_value(ipv4_remote) {
                info!(ctx, "inbound: error writing ipv4_remote");
            }
            if let Err(_) = self.write_value(u16::from_be(port_remote)) {
                info!(ctx, "inbound: error writing port_remote");
            }
            if let Err(_) = self.write_value(ipv4_local) {
                info!(ctx, "inbound: error writing ipv4_local");
            }
            if let Err(_) = self.write_value(u16::from_be(port_local)) {
                info!(ctx, "inbound: error writing port_local");
            }
        }

        defs::FAMILY_SIZE + defs::IPV4_SIZE + defs::PORT_SIZE + defs::IPV4_SIZE + defs::PORT_SIZE
    }

    fn write_inet6_sock_tuple(
        &mut self,
        ctx: &FExitContext,
        sk: &Sock,
        is_outbound: bool,
        sockaddr: &Sockaddr,
        is_kern_sockaddr: bool,
    ) -> usize {
        let inet6_sk = sk.as_inet_sock();
        let ipv6_local = inet6_sk
            .pinet6()
            .and_then(|pinet6| pinet6.saddr().in6_u())
            .unwrap_or([0, 0, 0, 0]);
        let port_local = inet6_sk.inet_sport().unwrap_or(0);
        let mut ipv6_remote = sk
            .__sk_common()
            .skc_v6_daddr()
            .in6_u()
            .unwrap_or([0, 0, 0, 0]);
        let mut port_remote = sk.__sk_common().skc_dport().unwrap_or(0);

        // Kernel doesn't always fill sk->__sk_common in sendto and sendmsg syscalls (as in
        // the case of a UDP connection). We fall back to the address from userspace when
        // the kernel-provided address is NULL.
        if port_remote == 0 && !sockaddr.is_null() {
            let sockaddr = sockaddr.as_sockaddr_in6();
            if is_kern_sockaddr {
                ipv6_remote = sockaddr.sin6_addr().in6_u().unwrap_or([0, 0, 0, 0]);
                port_remote = sockaddr.sin6_port().unwrap_or(0);
            } else {
                ipv6_remote = sockaddr.sin6_addr().in6_u_user().unwrap_or([0, 0, 0, 0]);
                port_remote = sockaddr.sin6_port_user().unwrap_or(0);
            }
        }

        // Pack the tuple info: (sock_family, local_ipv6, local_port, remote_ipv6, remote_port)
        let _ = self.write_value(scap::encode_socket_family(defs::AF_INET6));
        if is_outbound {
            let _ = self.write_value(ipv6_local);
            let _ = self.write_value(u16::from_be(port_local));
            let _ = self.write_value(ipv6_remote);
            let _ = self.write_value(u16::from_be(port_remote));
        } else {
            let _ = self.write_value(ipv6_remote);
            let _ = self.write_value(u16::from_be(port_remote));
            let _ = self.write_value(ipv6_local);
            let _ = self.write_value(u16::from_be(port_local));
        }

        defs::FAMILY_SIZE + defs::IPV6_SIZE + defs::PORT_SIZE + defs::IPV6_SIZE + defs::PORT_SIZE
    }

    fn write_unix_sock_tuple(
        &mut self,
        ctx: &FExitContext,
        sk: &Sock,
        is_outbound: bool,
        sockaddr: &Sockaddr,
        is_kern_sockaddr: bool,
    ) -> usize {
        let sk_local = sk.as_unix_sock();

        let sk_peer = sk_local.peer().unwrap_or(Sock::wrap(null_mut()));
        let sk_peer = sk_peer.as_unix_sock();

        let mut path: [c_uchar; defs::UNIX_PATH_MAX] = [0; defs::UNIX_PATH_MAX];
        let path_mut = &mut path;

        // Pack the tuple info: (sock_family, dest_os_ptr, src_os_ptr, dest_unix_path)
        let _ = self.write_value(scap::encode_socket_family(defs::AF_UNIX));
        if is_outbound {
            let _ = self.write_value(sk_peer.serialize_ptr() as u64);
            let _ = self.write_value(sk_local.serialize_ptr() as u64);
            if sk_peer.is_null() && !sockaddr.is_null() {
                let sockaddr = sockaddr.as_sockaddr_un();
                let _ = sockets::sockaddr_un_path_into(&sockaddr, is_kern_sockaddr, path_mut);
            } else if !sk_peer.is_null() {
                let _ = sockets::unix_sock_addr_path_into(&sk_peer, path_mut);
            }
        } else {
            let _ = self.write_value(sk_local.serialize_ptr() as u64);
            let _ = self.write_value(sk_peer.serialize_ptr() as u64);
            let _ = sockets::unix_sock_addr_path_into(&sk_local, path_mut);
        }

        let written_bytes = self.write_sockaddr_path(&path).unwrap_or(0);

        defs::FAMILY_SIZE + defs::KERNEL_POINTER + defs::KERNEL_POINTER + written_bytes as usize
    }

    pub fn save(self) -> AuxiliaryMapWriterState {
        AuxiliaryMapWriterState {
            remaining_lengths_room: self.lengths.len() as u8,
            remaining_values_room: self.values.len() as u64,
        }
    }
}

pub struct AuxiliaryMapWriterState {
    remaining_lengths_room: u8,
    remaining_values_room: u64,
}

pub struct AuxiliaryMap {
    // raw space to save our variable-size event.
    data: [u8; AUXILIARY_MAP_LEN],
    saved_writer_state: Option<AuxiliaryMapWriterState>,
}

impl AuxiliaryMap {
    fn header_mut_from_bytes(header: &mut [u8]) -> Result<&mut EventHeader, i64> {
        EventHeader::mut_from_bytes(header).map_err(|_| 1)
    }

    pub fn writer(
        &mut self,
        event_type: EventType,
        nparams: u8,
    ) -> Result<AuxiliaryMapWriter, i64> {
        let (header, bufs) = self.data.as_mut().split_at_mut(size_of::<EventHeader>());

        let lengths_len = (nparams as usize) * size_of::<u16>();
        if lengths_len > bufs.len() {
            return Err(1);
        }

        let header = Self::header_mut_from_bytes(header)?;
        header.ts = (shared_state::boot_time() + unsafe { bpf_ktime_get_boot_ns() }).into();
        header.tgid_pid = bpf_get_current_pid_tgid().into();
        header.nparams = (nparams as u32).into();
        header.evt_type = (event_type as u16).into();
        header.len = (size_of::<EventHeader>() as u32 + lengths_len as u32).into();

        let (lengths, values) = bufs.split_at_mut(lengths_len);

        self.saved_writer_state = None;
        Ok(AuxiliaryMapWriter {
            header,
            lengths,
            values,
        })
    }

    /// Save the writer state to enable resuming it later via `resume_writer()`.
    pub fn save_writer_state(&mut self, state: AuxiliaryMapWriterState) {
        self.saved_writer_state = Some(state);
    }

    /// Resume the writer state. It returns an error if the user didn't call `save_writer_state()`
    /// before.
    pub fn resume_writer(&mut self) -> Result<AuxiliaryMapWriter, i64> {
        let Some(AuxiliaryMapWriterState {
            remaining_values_room,
            remaining_lengths_room,
        }) = self.saved_writer_state.take()
        // This sets saved_writer_state to None.
        else {
            return Err(1);
        };

        let remaining_values_room = remaining_values_room as usize;
        let remaining_lengths_room = remaining_lengths_room as usize;
        if remaining_lengths_room < remaining_lengths_room
            || remaining_values_room > AUXILIARY_MAP_LEN
            || remaining_lengths_room > AUXILIARY_MAP_LEN
        {
            return Err(1);
        }

        let (header, bufs) = self.data.as_mut().split_at_mut(size_of::<EventHeader>());
        let header = Self::header_mut_from_bytes(header)?;
        let nparams = header.nparams.get();
        let bufs_len = bufs.len();
        let lengths_len = (nparams as usize) * size_of::<u16>();
        if lengths_len > bufs_len {
            return Err(1);
        }
        let values_len = bufs_len - lengths_len;
        if values_len > bufs_len {
            return Err(1);
        }
        if remaining_lengths_room >= lengths_len || remaining_values_room > values_len {
            return Err(1);
        }
        let lengths_offset = lengths_len - remaining_lengths_room;
        let values_offset = values_len - remaining_values_room;
        if lengths_offset > values_offset {
            return Err(1);
        }

        let (mut lengths, mut values) = bufs.as_mut().split_at_mut(lengths_len);
        forget(&mut lengths, lengths_offset);
        forget(&mut values, values_offset);
        Ok(AuxiliaryMapWriter {
            header,
            lengths,
            values,
        })
    }

    pub fn header(&self) -> Result<&EventHeader, i64> {
        EventHeader::ref_from_bytes(&self.data[..size_of::<EventHeader>()]).map_err(|_| 1)
    }

    pub fn as_bytes(&mut self) -> Result<&[u8], i64> {
        let len = self.header()?.len.get() as usize;
        if len > self.data.len() {
            return Err(1);
        }
        Ok(&self.data[..len])
    }
}
