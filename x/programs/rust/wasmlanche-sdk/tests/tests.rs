#![feature(strict_provenance)]
use sptr::Strict;

use std::{
    path::{Path, PathBuf},
    process::Command,
};
use wasmlanche_sdk::{
    memory::{into_bytes, split_host_ptr},
    Context, HostPtr, Program,
};
use wasmtime::{AsContext, Instance, Module, Store, TypedFunc};

const WASM_TARGET: &str = "wasm32-unknown-unknown";
const TEST_PKG: &str = "test-crate";
const PROFILE: &str = "release";

#[test]
fn public_functions() {
    let wasm_path = {
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
        let manifest_dir = std::path::Path::new(&manifest_dir);
        let test_crate_dir = manifest_dir.join("tests").join(TEST_PKG);
        let target_dir = std::env::var("CARGO_TARGET_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| manifest_dir.join("target"));

        let status = Command::new("cargo")
            .arg("build")
            .arg("--package")
            .arg(TEST_PKG)
            .arg("--target")
            .arg(WASM_TARGET)
            .arg("--profile")
            .arg(PROFILE)
            .arg("--target-dir")
            .arg(&target_dir)
            .current_dir(&test_crate_dir)
            .status()
            .expect("cargo build failed");

        if !status.success() {
            panic!("cargo build failed");
        }

        target_dir
            .join(WASM_TARGET)
            .join(PROFILE)
            .join(TEST_PKG.replace('-', "_"))
            .with_extension("wasm")
    };

    let mut test_crate = TestCrate::new(wasm_path);

    let context_ptr = {
        let program_id: [u8; Program::LEN] = std::array::from_fn(|_| 1);
        // this is a hack to create a program since the constructor is private
        let program: Program =
            borsh::from_slice(&program_id).expect("the program should deserialize");
        let context = Context { program };
        let serialized_context = borsh::to_vec(&context).expect("failed to serialize context");

        test_crate.allocate(serialized_context)
    };

    assert!(test_crate.always_true(context_ptr));

    let combined_binary_digits = test_crate.combine_last_bit_of_each_id_byte(context_ptr);
    assert_eq!(combined_binary_digits, u32::MAX);
}

#[test]
fn into_bytes_ub() {
    let wasm_path = {
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
        let manifest_dir = std::path::Path::new(&manifest_dir);
        let test_crate_dir = manifest_dir.join("tests").join(TEST_PKG);
        let target_dir = std::env::var("CARGO_TARGET_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| manifest_dir.join("target"));

        // let status = Command::new("cargo")
        //     .arg("+nightly")
        //     .arg("build")
        //     .arg("--package")
        //     .arg(TEST_PKG)
        //     .arg("--target")
        //     .arg(WASM_TARGET)
        //     .arg("--profile")
        //     .arg(PROFILE)
        //     .arg("--target-dir")
        //     .arg(&target_dir)
        //     .current_dir(&test_crate_dir)
        //     .status()
        //     .expect("cargo build failed");

        // if !status.success() {
        //     panic!("cargo build failed");
        // }

        target_dir
            .join(WASM_TARGET)
            .join(PROFILE)
            .join(TEST_PKG.replace('-', "_"))
            .with_extension("wasm")
    };

    let mut test_crate = TestCrate::new(wasm_path);

    // valid context ptr
    let context_ptr = {
        let program_id: [u8; Program::LEN] = std::array::from_fn(|_| 1);
        // this is a hack to create a program since the constructor is private
        let program: Program =
            borsh::from_slice(&program_id).expect("the program should deserialize");
        let context = Context { program };
        let serialized_context = borsh::to_vec(&context).expect("failed to serialize context");

        test_crate.allocate(serialized_context)
    };

    let base = test_crate.mem_data_ptr();
    let (alloc_offset, len) = split_host_ptr(context_ptr);
    let offset = unsafe { base.offset(alloc_offset.try_into().unwrap()) };
    let addr = offset.expose_addr();
    dbg!(&addr, len);
    let context_ptr = addr as i64 | (len << 32) as i64;

    // assert!(test_crate.always_true(context_ptr));
    // let len = (context_ptr >> 32) as usize;
    // let mut buf: Vec<u8> = (0..len).map(|_| 0).collect();
    // let buf = &mut buf;
    // let data = test_crate.read_at((context_ptr & !0u32 as i64) as usize, buf);
    // dbg!(&data);
    // let context_ptr = offset;
    let data = into_bytes(context_ptr);
    dbg!(&data);
    // panic!();

    // let result = std::panic::catch_unwind(|| {
    //     let _ = into_bytes(context_ptr);
    // });
    // assert!(result.is_ok());
}

type AllocParam = i32;
type AllocReturn = i32;

struct TestCrate {
    store: Store<()>,
    instance: Instance,
    allocate_func: TypedFunc<AllocParam, AllocReturn>,
    always_true_func: TypedFunc<HostPtr, i64>,
    combine_last_bit_of_each_id_byte_func: TypedFunc<HostPtr, u32>,
}

impl TestCrate {
    fn new(wasm_path: impl AsRef<Path>) -> Self {
        let mut store: Store<()> = Store::default();
        let module = Module::from_file(store.engine(), wasm_path).expect("failed to load wasm");
        let instance = Instance::new(&mut store, &module, &[]).expect("failed to instantiate wasm");

        let allocate_func = instance
            .get_typed_func::<AllocParam, AllocReturn>(&mut store, "alloc")
            .expect("failed to find `alloc` function");

        let always_true_func = instance
            .get_typed_func::<i64, i64>(&mut store, "always_true_guest")
            .expect("failed to find `always_true` function");
        let combine_last_bit_of_each_id_byte_func = instance
            .get_typed_func::<i64, u32>(&mut store, "combine_last_bit_of_each_id_byte_guest")
            .expect("combine_last_bit_of_each_id_byte should be a function");

        Self {
            store,
            instance,
            allocate_func,
            always_true_func,
            combine_last_bit_of_each_id_byte_func,
        }
    }

    fn allocate(&mut self, data: Vec<u8>) -> HostPtr {
        let offset = self
            .allocate_func
            .call(&mut self.store, data.len() as i32)
            .expect("failed to allocate memory");

        let memory = self
            .instance
            .get_memory(&mut self.store, "memory")
            .expect("failed to get memory");

        memory
            .write(&mut self.store, offset as usize, &data)
            .expect("failed to write data to memory");

        ((data.len() as HostPtr) << 32) | offset as HostPtr
    }

    fn mem_data_ptr(&mut self) -> *const u8 {
        let memory = self
            .instance
            .get_memory(&mut self.store, "memory")
            .expect("failed to get memory");

        let data = memory.data(self.store.as_context());
        data.as_ptr()
    }

    // fn read_at<'a>(&mut self, offset: usize, buf: &'a mut [u8]) -> &'a mut [u8] {
    //     // let buf = &mut [0; len];
    //     // let mut buf: Vec<u8> = (0..len).map(|_| 0).collect();
    //     // let buf = &mut buf;
    //     // let buf = &mut [0; 32];

    //     let memory = self
    //         .instance
    //         .get_memory(&mut self.store, "memory")
    //         .expect("failed to get memory");

    //     memory
    //         .read(&mut self.store, offset, buf)
    //         .expect("failed to write data to memory");

    //     buf
    // }

    fn always_true(&mut self, ptr: HostPtr) -> bool {
        self.always_true_func
            .call(&mut self.store, ptr)
            .expect("failed to call `always_true` function")
            == true as i64
    }

    fn combine_last_bit_of_each_id_byte(&mut self, ptr: HostPtr) -> u32 {
        self.combine_last_bit_of_each_id_byte_func
            .call(&mut self.store, ptr)
            .expect("failed to call `combine_last_bit_of_each_id_byte` function")
    }
}
