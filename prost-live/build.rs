use prost_build::Config;
fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rerun-if-changed=person.proto");

    Config::new()
        .out_dir("src/pb")
        // .bytes(&["."])
        .btree_map(&["Person.scores"])
        .type_attribute(".", "#[derive(serde::Serialize, serde::Deserialize)]")
        .field_attribute("Person.data", "#[serde(skip_serializing_if=\"Vec::is_empty\")]")
        .compile_protos(&["person.proto"], &["."])
        .unwrap();

}
