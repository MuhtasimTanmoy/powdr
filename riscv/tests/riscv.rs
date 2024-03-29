mod common;

use common::verify_riscv_asm_string;
use mktemp::Temp;
use powdr_backend::BackendType;
use powdr_number::{FieldElement, GoldilocksField};
use powdr_pipeline::{test_util::verify_asm_string, verify::verify, Pipeline};
use std::path::PathBuf;
use test_log::test;

use powdr_riscv::{
    continuations::{rust_continuations, rust_continuations_dry_run},
    Runtime,
};

/// Compiles and runs a rust file with continuations, runs the full
/// witness generation & verifies it using Pilcom.
pub fn test_continuations(case: &str) {
    let rust_file = format!("{case}.rs");
    let runtime = Runtime::base().with_poseidon();
    let temp_dir = Temp::new_dir().unwrap();
    let riscv_asm =
        powdr_riscv::compile_rust_to_riscv_asm(&format!("tests/riscv_data/{rust_file}"), &temp_dir);
    let powdr_asm = powdr_riscv::compiler::compile::<GoldilocksField>(riscv_asm, &runtime, true);

    // Manually create tmp dir, so that it is the same in all chunks.
    let tmp_dir = mktemp::Temp::new_dir().unwrap();

    let mut pipeline = Pipeline::<GoldilocksField>::default()
        .from_asm_string(powdr_asm.clone(), Some(PathBuf::from(&rust_file)))
        .with_prover_inputs(Default::default())
        .with_output(tmp_dir.to_path_buf(), false);
    let pipeline_callback = |pipeline: Pipeline<GoldilocksField>| -> Result<(), ()> {
        // Can't use `verify_pipeline`, because the pipeline was renamed in the middle of after
        // computing the constants file.
        let mut pipeline = pipeline.with_backend(BackendType::PilStarkCli);
        pipeline.compute_proof().unwrap();
        verify(pipeline.output_dir().unwrap(), pipeline.name(), Some(case)).unwrap();
        Ok(())
    };
    let bootloader_inputs = rust_continuations_dry_run(&mut pipeline);
    rust_continuations(pipeline, pipeline_callback, bootloader_inputs).unwrap();
}

#[test]
#[ignore = "Too slow"]
fn test_trivial() {
    let case = "trivial.rs";
    verify_riscv_file(case, Default::default(), &Runtime::base())
}

#[test]
#[ignore = "Too slow"]
fn test_zero_with_values() {
    let case = "zero_with_values.rs";
    verify_riscv_file(case, Default::default(), &Runtime::base())
}

#[test]
#[ignore = "Too slow"]
fn test_poseidon_gl() {
    let case = "poseidon_gl_via_coprocessor.rs";
    verify_riscv_file(case, Default::default(), &Runtime::base().with_poseidon());
}

#[test]
#[ignore = "Too slow"]
fn test_sum() {
    let case = "sum.rs";
    verify_riscv_file(
        case,
        [16, 4, 1, 2, 8, 5].iter().map(|&x| x.into()).collect(),
        &Runtime::base(),
    );
}

#[test]
#[ignore = "Too slow"]
fn test_byte_access() {
    let case = "byte_access.rs";
    verify_riscv_file(
        case,
        [0, 104, 707].iter().map(|&x| x.into()).collect(),
        &Runtime::base(),
    );
}

#[test]
#[ignore = "Too slow"]
fn test_double_word() {
    let case = "double_word.rs";
    let a0 = 0x01000000u32;
    let a1 = 0x010000ffu32;
    let b0 = 0xf100b00fu32;
    let b1 = 0x0100f0f0u32;
    let c = ((a0 as u64) | ((a1 as u64) << 32)).wrapping_mul((b0 as u64) | ((b1 as u64) << 32));
    verify_riscv_file(
        case,
        [
            a0,
            a1,
            b0,
            b1,
            (c & 0xffffffff) as u32,
            ((c >> 32) & 0xffffffff) as u32,
        ]
        .iter()
        .map(|&x| x.into())
        .collect(),
        &Runtime::base(),
    );
}

#[test]
#[ignore = "Too slow"]
fn test_memfuncs() {
    let case = "memfuncs";
    verify_riscv_crate(case, Default::default(), &Runtime::base());
}

#[test]
#[ignore = "Too slow"]
fn test_keccak() {
    let case = "keccak";
    verify_riscv_crate(case, Default::default(), &Runtime::base());
}

#[test]
#[ignore = "Too slow"]
fn test_vec_median() {
    let case = "vec_median";
    verify_riscv_crate(
        case,
        [5, 11, 15, 75, 6, 5, 1, 4, 7, 3, 2, 9, 2]
            .into_iter()
            .map(|x| x.into())
            .collect(),
        &Runtime::base(),
    );
}

#[test]
#[ignore = "Too slow"]
fn test_password() {
    let case = "password_checker";
    verify_riscv_crate(case, Default::default(), &Runtime::base());
}

#[test]
#[ignore = "Too slow"]
fn test_function_pointer() {
    let case = "function_pointer";
    verify_riscv_crate(
        case,
        [2734, 735, 1999].into_iter().map(|x| x.into()).collect(),
        &Runtime::base(),
    );
}

/*
mstore(0, 666)
return(0, 32)
*/
#[cfg(feature = "complex-tests")]
static BYTECODE: &str = "61029a60005260206000f3";

#[cfg(feature = "complex-tests")]
#[ignore = "Too slow"]
#[test]
fn test_evm() {
    let case = "evm";
    let bytes = hex::decode(BYTECODE).unwrap();

    verify_riscv_crate_with_data(case, vec![], &Runtime::base(), vec![(666, bytes)]);
}

#[ignore = "Too slow"]
#[test]
fn test_sum_serde() {
    let case = "sum_serde";

    let data: Vec<u32> = vec![1, 2, 8, 5];
    let answer = data.iter().sum::<u32>();

    verify_riscv_crate_with_data(
        case,
        vec![answer.into()],
        &Runtime::base(),
        vec![(42, data)],
    );
}

#[test]
#[ignore = "Too slow"]
#[should_panic(expected = "Witness generation failed.")]
fn test_print() {
    let case = "print.rs";
    verify_file(case, Default::default(), &Runtime::base());
}

#[test]
fn test_many_chunks_dry() {
    // Compiles and runs the many_chunks.rs example with continuations, just computing
    // and validating the bootloader inputs.
    // Doesn't do a full witness generation, verification, or proving.
    let case = "many_chunks.rs";
    let runtime = Runtime::base().with_poseidon();
    let temp_dir = Temp::new_dir().unwrap();
    let riscv_asm =
        powdr_riscv::compile_rust_to_riscv_asm(&format!("tests/riscv_data/{case}"), &temp_dir);
    let powdr_asm = powdr_riscv::compiler::compile::<GoldilocksField>(riscv_asm, &runtime, true);

    let mut pipeline = Pipeline::default()
        .from_asm_string(powdr_asm, Some(PathBuf::from(case)))
        .with_prover_inputs(Default::default());
    rust_continuations_dry_run::<GoldilocksField>(&mut pipeline);
}

#[test]
#[ignore = "Too slow"]
fn test_many_chunks() {
    test_continuations("many_chunks")
}

#[test]
#[ignore = "Too slow"]
fn test_many_chunks_memory() {
    test_continuations("many_chunks_memory")
}

fn verify_file(case: &str, inputs: Vec<GoldilocksField>, runtime: &Runtime) {
    let temp_dir = Temp::new_dir().unwrap();
    let riscv_asm =
        powdr_riscv::compile_rust_to_riscv_asm(&format!("tests/riscv_data/{case}"), &temp_dir);
    let powdr_asm = powdr_riscv::compiler::compile::<GoldilocksField>(riscv_asm, runtime, false);

    verify_asm_string::<()>(&format!("{case}.asm"), &powdr_asm, inputs, vec![], None);
}

#[test]
#[ignore = "Too slow"]
#[should_panic(
    expected = "called `Result::unwrap()` on an `Err` value: \"Error accessing prover inputs: Index 0 out of bounds 0\""
)]
fn test_print_rv32_executor() {
    let case = "print.rs";
    verify_riscv_file(case, Default::default(), &Runtime::base());
}

fn verify_riscv_file(case: &str, inputs: Vec<GoldilocksField>, runtime: &Runtime) {
    let temp_dir = Temp::new_dir().unwrap();
    let riscv_asm =
        powdr_riscv::compile_rust_to_riscv_asm(&format!("tests/riscv_data/{case}"), &temp_dir);
    let powdr_asm = powdr_riscv::compiler::compile::<GoldilocksField>(riscv_asm, runtime, false);

    verify_riscv_asm_string::<()>(&format!("{case}.asm"), &powdr_asm, inputs, None);
}

fn verify_riscv_crate(case: &str, inputs: Vec<GoldilocksField>, runtime: &Runtime) {
    let powdr_asm = compile_riscv_crate::<GoldilocksField>(case, runtime);

    verify_riscv_asm_string::<()>(&format!("{case}.asm"), &powdr_asm, inputs, None);
}

fn verify_riscv_crate_with_data<S: serde::Serialize + Send + Sync + 'static>(
    case: &str,
    inputs: Vec<GoldilocksField>,
    runtime: &Runtime,
    data: Vec<(u32, S)>,
) {
    let powdr_asm = compile_riscv_crate::<GoldilocksField>(case, runtime);

    verify_riscv_asm_string(&format!("{case}.asm"), &powdr_asm, inputs, Some(data));
}

fn compile_riscv_crate<T: FieldElement>(case: &str, runtime: &Runtime) -> String {
    let temp_dir = Temp::new_dir().unwrap();
    let riscv_asm = powdr_riscv::compile_rust_crate_to_riscv_asm(
        &format!("tests/riscv_data/{case}/Cargo.toml"),
        &temp_dir,
    );
    powdr_riscv::compiler::compile::<T>(riscv_asm, runtime, false)
}
