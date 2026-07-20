use std::process::Command;

#[test]
fn server_rejects_legacy_or_partial_database_credentials() {
    for envs in [
        vec![("DATABASE_URL", "postgres://legacy.invalid/memphant")],
        vec![(
            "MEMPHANT_APP_DATABASE_URL",
            "postgres://partial.invalid/memphant",
        )],
    ] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_memphant-server"));
        command
            .env_remove("DATABASE_URL")
            .env_remove("MEMPHANT_APP_DATABASE_URL")
            .env_remove("MEMPHANT_AUTHN_DATABASE_URL");
        for (name, value) in envs {
            command.env(name, value);
        }
        let output = command.output().expect("memphant-server binary runs");
        assert!(!output.status.success());
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("database config requires"),
            "stderr={}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
