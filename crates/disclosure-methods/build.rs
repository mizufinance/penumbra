fn main() {
    std::env::set_var("RISC0_BUILD_LOCKED", "1");
    risc0_build::embed_methods();
}
