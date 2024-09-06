// Copyright 2024 RISC Zero, Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::{
    collections::HashMap,
    env,
    fs::{self, File},
    io::BufReader,
    process::Command,
};

use risc0_build::{embed_methods_with_options, DockerOptions, GuestOptions};
use risc0_build_ethereum::generate_solidity_files;
use serde_json::Value;

// Paths where the generated Solidity files will be written.
const SOLIDITY_IMAGE_ID_PATH: &str = "../contracts/src/ImageID.sol";
const SOLIDITY_ELF_PATH: &str = "../contracts/test/Elf.sol";

fn main() {
    // Builds can be made deterministic, and thereby reproducible, by using Docker to build the
    // guest. Check the RISC0_USE_DOCKER variable and use Docker to build the guest if set.
    let use_docker = env::var("RISC0_USE_DOCKER").ok().map(|_| DockerOptions {
        root_dir: Some("../".into()),
    });

    // Generate Rust source files for the methods crate.
    let guests = embed_methods_with_options(HashMap::from([(
        "light-client-guest",
        GuestOptions {
            features: Vec::new(),
            use_docker,
        },
    )]));

    // Generate Solidity source files for use with Forge.
    let solidity_opts = risc0_build_ethereum::Options::default()
        .with_image_id_sol_path(SOLIDITY_IMAGE_ID_PATH)
        .with_elf_sol_path(SOLIDITY_ELF_PATH);

    generate_solidity_files(guests.as_slice(), &solidity_opts).unwrap();

    let contracts_dir = fs::canonicalize(env!("CARGO_MANIFEST_DIR"))
        .unwrap()
        // Go back a directory from `./light-client-guest`
        .parent()
        .unwrap()
        // Use guest directory.
        .join("contracts");

    // Rebuild contracts after generating image ID to avoid inconsistencies.
    Command::new("forge")
        .args(["build", "--silent", "--via-ir"])
        .current_dir(&contracts_dir)
        .status()
        .unwrap();

    // Read and deserialize JSON artifact
    let file = File::open(
        contracts_dir
            .clone()
            .join("out")
            .join("Blobstream0.sol")
            .join("Blobstream0.json"),
    )
    .expect("Failed to open JSON file");
    let reader = BufReader::new(file);
    let artifact: Value = serde_json::from_reader(reader).expect("Failed to parse JSON");

    // Write the artifact to artifacts dir
    // Open the file for writing
    let output_file = File::create(contracts_dir.join("artifacts").join("Blobstream0.json"))
        .expect("Failed to create output file");

    // Write the formatted JSON
    serde_json::to_writer_pretty(output_file, &artifact).expect("Failed to write formatted JSON");

    // NOTE: This should not be a circular update, as the code files are not updated with this, just
    //       the built artifact that is pointed to.
    println!(
        "cargo:rerun-if-changed={}",
        contracts_dir.join("src").display()
    );
    println!("cargo:rerun-if-env-changed=RISC0_USE_DOCKER");
}
