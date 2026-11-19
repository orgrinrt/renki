#[test]
fn probe() {
    let out = std::process::Command::new("./probe.sh")
        .arg("/home/user/target/locktest")
        .output()
        .unwrap();
    std::fs::write("in-cargo-test.txt", out.stdout).unwrap();
}
