clippy-strict:
    cargo clippy -- -Dclippy::all -Dclippy::pedantic -Wclippy::unwrap_used -Wclippy::expect_used -Wclippy::nursery

update-dependencies:
    cargo upgrade -i allow && cargo update    
