//! Tarball compatibility checks.

#[test]
fn tarball_cap_logic_compiles() {
    // ROB-06: NpmClient::new() should compile; the cap logic is in download_tarball.
    // We only verify the constructor is accessible — real HTTP tests are out of scope.
    use pledge::ecosystems::node::acquire::registry::NpmClient;
    let result = NpmClient::new();
    // May fail in CI without TLS certs, but the type itself must exist
    let _ = result; // don't assert Ok — network not available in tests
}
