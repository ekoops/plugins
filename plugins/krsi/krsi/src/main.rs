use krsi_common::flags::{FeatureFlags, OpFlags};

mod ebpf;
mod krsi_event;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    env_logger::init();
    let feature_flags = FeatureFlags::IO_URING;
    // let op_flags = OpFlags::BIND | OpFlags::LINKAT | OpFlags::MKDIRAT | OpFlags::OPEN | OpFlags::RENAMEAT | OpFlags::SOCKET | OpFlags::SYMLINKAT | OpFlags::UNLINKAT;
    let op_flags = OpFlags::CONNECT;
    let mut ebpf = ebpf::Ebpf::try_new(true, feature_flags, op_flags).unwrap();
    ebpf.load_and_attach_programs().unwrap();
    let mut ring_buf = ebpf.ring_buffer().unwrap();
    loop {
        if let Some(item) = ring_buf.next() {
            let buf = &*item;
            if let Ok(event) = krsi_event::parse_ringbuf_event(buf) {
                println!("{event:?}");
            }
        }
    }
}
