# Ultra-strict clippy: deny pedantic, nursery, and unsafe lints
# -D = deny (error), -W = warn, -A = allow
# Remove redundant -Dclippy::all since pedantic includes it
clippy-strict:
    cargo clippy -- \
        -Dclippy::pedantic \
        -Dclippy::nursery \
        -Dclippy::unwrap_used \
        -Dclippy::expect_used \
        -Aclippy::missing_docs_in_private_items

update-dependencies:
    cargo upgrade -i allow && cargo update    
