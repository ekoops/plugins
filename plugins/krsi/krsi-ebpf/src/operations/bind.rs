// SPDX-License-Identifier: Apache-2.0
/*
Copyright (C) 2025 The Falco Authors.

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

    http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.
*/

//! # Data extraction
//!
//! ## Kernel functions call graph (`bind` syscall path)
//! ```
//! SYSCALL_DEFINE3(bind, int, fd, struct sockaddr __user *, umyaddr, int, addrlen)
//!     int __sys_bind(int fd, struct sockaddr __user *umyaddr, int addrlen)
//!         int __sys_bind_socket(struct socket *sock, struct sockaddr_storage *address,
//!             int addrlen)
//! ```
//!
//! ## Kernel functions call graph (`socketcall` syscall path)
//! ```
//! SYSCALL_DEFINE2(socketcall, int, call, unsigned long __user *, args)
//!     int __sys_bind(int fd, struct sockaddr __user *umyaddr, int addrlen)
//!         int __sys_bind_socket(struct socket *sock, struct sockaddr_storage *address,
//!             int addrlen)
//! ```
//!
//! ## Kernel function call graph (`io_uring` path)
//! ```
//! int io_bind(struct io_kiocb *req, unsigned int issue_flags)
//!     int __sys_bind_socket(struct socket *sock, struct sockaddr_storage *address, int addrlen)
//! ```
//!
//! ## Extraction flow
//! 1. `fentry:io_bind`
//! 2. `fexit:io_bind` | `fexit:__sys_bind`

use aya_ebpf::{
    macros::{fentry, fexit},
    programs::{FEntryContext, FExitContext},
    EbpfContext,
};
use krsi_common::EventType;
use krsi_ebpf_core::{wrap_arg, IoAsyncMsghdr, IoKiocb, Sockaddr};

use crate::{
    get_event_num_params, iouring, shared_state,
    shared_state::op_info::{BindData, OpInfo},
    submit_event, FileDescriptor,
};

#[fentry]
fn io_bind_e(ctx: FEntryContext) -> u32 {
    try_io_bind_e(ctx).unwrap_or(1)
}

fn try_io_bind_e(ctx: FEntryContext) -> Result<u32, i64> {
    let pid = ctx.pid();
    let req: IoKiocb = wrap_arg(unsafe { ctx.arg(0) });
    let file_descriptor = iouring::io_kiocb_cqe_file_descriptor(&req)?;
    let op_info = OpInfo::Bind(BindData { file_descriptor });
    shared_state::op_info::insert(pid, &op_info)
}

#[fexit]
fn io_bind_x(ctx: FExitContext) -> u32 {
    try_io_bind_x(ctx).unwrap_or(1)
}

fn try_io_bind_x(ctx: FExitContext) -> Result<u32, i64> {
    let pid = ctx.pid();
    let Some(OpInfo::Bind(BindData { file_descriptor })) =
        (unsafe { shared_state::op_info::get(pid) })
    else {
        return Err(1);
    };

    let _ = shared_state::op_info::remove(pid);

    let auxmap = shared_state::auxiliary_map().ok_or(1)?;
    const EVT_TYPE: EventType = EventType::Bind;
    let mut writer = auxmap.writer(EVT_TYPE, get_event_num_params(EVT_TYPE))?;

    // Parameter 1: iou_ret.
    let iou_ret: i64 = unsafe { ctx.arg(2) };
    writer.push(iou_ret);

    // Parameter 2: res.
    let req: IoKiocb = wrap_arg(unsafe { ctx.arg(0) });
    match iouring::io_kiocb_cqe_res(&req, iou_ret) {
        Ok(Some(cqe_res)) => writer.push(cqe_res as i64),
        _ => writer.push_empty(),
    }

    // Parameter 3: addr.
    match req.async_data_as::<IoAsyncMsghdr>() {
        Ok(io) => writer.push_sockaddr(&io.addr(), true),
        Err(_) => writer.push_empty(),
    };

    // Parameter 4: fd.
    // Parameter 5: file_index.
    writer.push_file_descriptor(*file_descriptor);

    let event = auxmap.as_bytes()?;
    submit_event(event);
    Ok(0)
}

#[fexit]
#[allow(non_snake_case)]
fn __sys_bind_x(ctx: FExitContext) -> u32 {
    try___sys_bind_x(ctx).unwrap_or(1)
}

#[allow(non_snake_case)]
fn try___sys_bind_x(ctx: FExitContext) -> Result<u32, i64> {
    let auxmap = shared_state::auxiliary_map().ok_or(1)?;
    const EVT_TYPE: EventType = EventType::Bind;
    let mut writer = auxmap.writer(EVT_TYPE, get_event_num_params(EVT_TYPE))?;

    // Parameter 1: iou_ret.
    writer.push_empty();

    // Parameter 2: res.
    let res: i64 = unsafe { ctx.arg(3) };
    writer.push(res);

    // Parameter 3: addr.
    let sockaddr: Sockaddr = wrap_arg(unsafe { ctx.arg(1) });
    writer.push_sockaddr(&sockaddr, false);

    // Parameter 4: fd.
    // Parameter 5: file_index.
    let fd = unsafe { ctx.arg(0) };
    let file_descriptor = FileDescriptor::Fd(fd);
    writer.push_file_descriptor(file_descriptor);

    let event = auxmap.as_bytes()?;
    submit_event(event);
    Ok(0)
}
