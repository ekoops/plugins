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
//! ## Kernel functions call graph (`connect` and `socketcall` syscalls path)
//! ```
//! int __sys_connect(int fd, struct sockaddr __user *uservaddr, int addrlen)
//!     int __sys_connect_file(struct file *file, struct sockaddr_storage *address, int addrlen,
//!         int file_flags)
//! ```
//!
//! ## Kernel function call graph (`io_uring` path)
//! ```
//! int io_connect(struct io_kiocb *req, unsigned int issue_flags)
//!     int __sys_connect_file(struct file *file, struct sockaddr_storage *address, int addrlen,
//!         int file_flags)
//! ```
//!
//! ## Extraction flow
//! 1. `fentry:io_connect` | `fentry:__sys_connect`
//! 2. `fexit:__sys_connect_file`
//! 3. `fexit:io_connect` | `fexit:__sys_connect`

use core::ptr::null_mut;

use aya_ebpf::{
    cty::c_int,
    macros::{fentry, fexit},
    programs::{FEntryContext, FExitContext},
    EbpfContext,
};
use aya_log_ebpf::info;
use krsi_common::EventType;
use krsi_ebpf_core::{wrap_arg, File, IoKiocb, Sockaddr, Socket, Wrap};

use crate::{
    defs, get_event_num_params, iouring, shared_state,
    shared_state::op_info::{ConnectData, OpInfo},
    submit_event, FileDescriptor,
};

const EVT_TYPE: EventType = EventType::Connect;

#[fentry]
fn io_connect_e(ctx: FEntryContext) -> u32 {
    try_io_connect_e(ctx).unwrap_or(1)
}

fn try_io_connect_e(ctx: FEntryContext) -> Result<u32, i64> {
    let pid = ctx.pid();
    let req: IoKiocb = wrap_arg(unsafe { ctx.arg(0) });
    let file_descriptor = iouring::io_kiocb_cqe_file_descriptor(&req)?;
    let op_info = OpInfo::Connect(ConnectData {
        file_descriptor,
        is_iou: true,
        socktuple_len: 0,
    });
    shared_state::op_info::insert(pid, &op_info)
}

#[fentry]
#[allow(non_snake_case)]
fn __sys_connect_e(ctx: FEntryContext) -> u32 {
    try___sys_connect_e(ctx).unwrap_or(1)
}

#[allow(non_snake_case)]
fn try___sys_connect_e(ctx: FEntryContext) -> Result<u32, i64> {
    let pid = ctx.pid();
    let fd: i32 = unsafe { ctx.arg(0) };
    let op_info = OpInfo::Connect(ConnectData {
        file_descriptor: FileDescriptor::Fd(fd),
        is_iou: false,
        socktuple_len: 0,
    });
    shared_state::op_info::insert(pid, &op_info)
}

#[fexit]
#[allow(non_snake_case)]
fn __sys_connect_file_x(ctx: FExitContext) -> u32 {
    try___sys_connect_file_x(ctx).unwrap_or(1)
}

#[allow(non_snake_case)]
fn try___sys_connect_file_x(ctx: FExitContext) -> Result<u32, i64> {
    let pid = ctx.pid();
    let Some(OpInfo::Connect(op_data)) = (unsafe { shared_state::op_info::get_mut(pid) }) else {
        return Ok(0);
    };

    let auxmap = shared_state::auxiliary_map().ok_or(1)?;
    let mut writer = auxmap.writer(EVT_TYPE, get_event_num_params(EVT_TYPE))?;

    let ret: c_int = unsafe { ctx.arg(4) };

    // Parameter 1: tuple.
    let socktuple_len = if ret == 0 || ret == -defs::EINPROGRESS {
        let file: File = wrap_arg(unsafe { ctx.arg(0) });
        let sock = Socket::wrap(file.private_data().unwrap_or(null_mut()).cast());
        let sockaddr: Sockaddr = wrap_arg(unsafe { ctx.arg(1) });
        writer.push_sock_tuple(&ctx, &sock, true, &sockaddr, true)
    } else {
        writer.push_empty();
        0
    };

    if op_data.is_iou {
        op_data.socktuple_len = socktuple_len;
        let writer_state = writer.save();
        auxmap.save_writer_state(writer_state);
        return Ok(0);
    }

    // Parameter 2: iou_ret.
    writer.push_empty();

    // Parameter 3: res.
    writer.push(ret as i64);

    // Parameter 4: fd.
    // Parameter 5: file_index.
    writer.push_file_descriptor(op_data.file_descriptor);

    let event = auxmap.as_bytes()?;
    submit_event(event);
    Ok(0)
}

#[fexit]
fn io_connect_x(ctx: FExitContext) -> u32 {
    try_io_connect_x(ctx).unwrap_or(1)
}

fn try_io_connect_x(ctx: FExitContext) -> Result<u32, i64> {
    let pid = ctx.pid();
    let Some(OpInfo::Connect(op_data)) = (unsafe { shared_state::op_info::get(pid) }) else {
        return Err(1);
    };

    let _ = shared_state::op_info::remove(pid);

    let auxmap = shared_state::auxiliary_map().ok_or(1)?;
    let mut writer = auxmap.resume_writer()?;

    // Parameter 1: tuple. (Already populated on fexit:__sys_connect_file)

    // Parameter 2: iou_ret.
    let iou_ret: i64 = unsafe { ctx.arg(2) };
    writer.push(iou_ret);
    //
    // // Parameter 3: res.
    // let req: IoKiocb = wrap_arg(unsafe { ctx.arg(0) });
    // match iouring::io_kiocb_cqe_res(&req, iou_ret) {
    //     Ok(Some(cqe_res)) => writer.push(cqe_res as i64),
    //     _ => writer.push_empty(),
    // }
    //
    // // Parameter 4: fd.
    // // Parameter 5: file_index.
    // writer.push_file_descriptor(op_data.file_descriptor);
    //
    let event = auxmap.as_bytes()?;
    info!(&ctx, "EVENT LEN: {}", event.len());
    submit_event(event);
    Ok(0)
}

#[fexit]
fn __sys_connect_x(ctx: FExitContext) -> u32 {
    let pid = ctx.pid();
    shared_state::op_info::remove(pid).unwrap_or(1)
}
