use alloc::sync::Arc;

use crate::kernel::object::directory::Directory;
use crate::kernel::object::handle::{
    AccessRights,
    HandleID,
};
use crate::kernel::object::invoke::{
    Invocation,
    InvocationError,
};
use crate::kernel::object::message::DirectoryMessage;
use crate::kernel::object::obj::{
    KernelHandleTable,
    KernelObject,
};
use crate::kernel::sync::RwLock;
use crate::{
    klog,
    klogln,
};

pub static PRINCIPAL_HANDLE_TABLE: RwLock<KernelHandleTable> = RwLock::new(KernelHandleTable::new());

pub static ROOT_DIRECTORY: RwLock<Option<HandleID>> = RwLock::new(None);

pub fn kernel_register_obj(obj: Arc<dyn KernelObject>, init_rights: AccessRights) -> HandleID {
    let mut table = PRINCIPAL_HANDLE_TABLE.write();
    table.insert(obj, init_rights)
}

pub fn sys_invoke(handle: HandleID, invocation: Invocation) -> Result<usize, InvocationError> {
    let demanded_rights = invocation.required_rights();

    let table = PRINCIPAL_HANDLE_TABLE.read();
    let obj_arc = table.resolve(handle, demanded_rights)?;
    drop(table);

    obj_arc.invoke(invocation)
}

pub fn sys_close(handle: HandleID) -> Result<(), InvocationError> {
    let mut table = PRINCIPAL_HANDLE_TABLE.write();
    table.close(handle)
}

pub fn sys_duplicate(handle: HandleID, requested_rights: AccessRights) -> Result<HandleID, InvocationError> {
    let mut table = PRINCIPAL_HANDLE_TABLE.write();
    let cloned_arc = table.resolve(handle, requested_rights)?;
    Ok(table.insert(cloned_arc, requested_rights))
}

pub fn debug_dump_handles() {
    let table = PRINCIPAL_HANDLE_TABLE.read();
    klogln!("{:#?}", *table);
}

#[derive(Debug)]
pub struct TestDevice {}

impl KernelObject for TestDevice {
    fn invoke(&self, invocation: Invocation) -> Result<usize, InvocationError> {
        match invocation {
            Invocation::Ping => klogln!("Pong!"),
            _ => return Err(InvocationError::UnsupportedOperation),
        }
        Ok(0)
    }
}

pub fn init_vfs() {
    let root_dir = Arc::new(Directory::new());
    let dev_dir = Arc::new(Directory::new());
    let test_device = Arc::new(TestDevice {});

    let root_handle = kernel_register_obj(root_dir, AccessRights::READ | AccessRights::WRITE);
    let dev_handle = kernel_register_obj(dev_dir, AccessRights::READ | AccessRights::WRITE);
    let test_handle = kernel_register_obj(test_device, AccessRights::READ | AccessRights::WRITE);

    *ROOT_DIRECTORY.write() = Some(root_handle);

    klog!("Mounting /dev ... ");
    // mount '/dev' inside '/'
    let dev_name = "dev";
    sys_invoke(
        root_handle,
        Invocation::Directory(DirectoryMessage::Link { name: dev_name.as_ptr(), name_len: dev_name.len(), handle_id: dev_handle }),
    )
    .expect("Failed to link /dev");
    klogln!("Mount success!");

    klog!("Mounting /dev/test ...");
    let test_name = "test";
    sys_invoke(
        dev_handle,
        Invocation::Directory(DirectoryMessage::Link { name: test_name.as_ptr(), name_len: test_name.len(), handle_id: test_handle }),
    )
    .expect("Failed to link /dev/test");
    klogln!("Mount success!");
}

pub fn test_vfs_path_res(path: &str) -> Result<HandleID, InvocationError> {
    let mut current_handle = ROOT_DIRECTORY.read().expect("Root not initialized.");

    for component in path.split('/') {
        if component.is_empty() {
            continue;
        }

        let result = sys_invoke(
            current_handle,
            Invocation::Directory(DirectoryMessage::Lookup { name: component.as_ptr(), name_len: component.len() }),
        );

        match result {
            Ok(next_handle_id) => {
                current_handle = HandleID(next_handle_id);
            }
            Err(e) => {
                klogln!("Path resolution failed at '{}': {:?}", component, e);
                return Err(e);
            }
        }
    }
    Ok(current_handle)
}

pub fn test_run() {
    let target_handle = test_vfs_path_res("/dev/test").unwrap();

    match sys_invoke(target_handle, Invocation::Ping) {
        Ok(_) => klogln!("Ping success!"),
        Err(_) => klogln!("Ping failed :("),
    }
}
