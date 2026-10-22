//! Teto real no Windows: o worker só recebe o pedido depois de entrar no Job Object.
use anyhow::{ensure, Context};
use std::os::windows::io::{FromRawHandle, OwnedHandle, RawHandle};
use windows_sys::Win32::System::JobObjects::*;

pub struct MemoryJob(#[allow(dead_code)] OwnedHandle);
impl MemoryJob {
    pub fn attach(child: RawHandle, bytes: u64) -> anyhow::Result<Self> {
        unsafe {
            let handle = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            ensure!(!handle.is_null(), "não foi possível criar o limite de memória");
            let job = Self(OwnedHandle::from_raw_handle(handle));
            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_PROCESS_MEMORY | JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            info.ProcessMemoryLimit = usize::try_from(bytes).context("limite de memória inválido")?;
            ensure!(
                SetInformationJobObject(
                    handle,
                    JobObjectExtendedLimitInformation,
                    &info as *const _ as _,
                    std::mem::size_of_val(&info) as u32
                ) != 0,
                "Windows recusou o limite de memória"
            );
            ensure!(AssignProcessToJobObject(handle, child) != 0, "Windows recusou isolar o modelo");
            Ok(job)
        }
    }
}

pub fn require_current_limit(maximum: u64) -> anyhow::Result<()> {
    unsafe {
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
        ensure!(
            QueryInformationJobObject(
                std::ptr::null_mut(),
                JobObjectExtendedLimitInformation,
                &mut info as *mut _ as _,
                std::mem::size_of_val(&info) as u32,
                std::ptr::null_mut()
            ) != 0,
            "worker local exige isolamento de memória"
        );
        ensure!(
            info.BasicLimitInformation.LimitFlags & JOB_OBJECT_LIMIT_PROCESS_MEMORY != 0
                && info.ProcessMemoryLimit > 0
                && info.ProcessMemoryLimit as u64 <= maximum,
            "worker local sem teto de 30% da RAM"
        );
        Ok(())
    }
}
