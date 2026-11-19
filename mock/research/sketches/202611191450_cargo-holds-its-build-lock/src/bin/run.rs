fn main() {
    let out = std::process::Command::new("./probe.sh")
        .arg("/home/user/target/locktest")
        .output()
        .unwrap();
    std::fs::write("in-cargo-run.txt", out.stdout).unwrap();
}
