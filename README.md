# rtime

## Requirement

- [rust](https://www.rust-lang.org/)
- cargo
- [cargo-make](https://github.com/sagiegurari/cargo-make) 

## Build

```
cargo make build-all  
```

## Enable Logging (necessary for getting outputs)

```
export RUST_LOG="debug"
```

## Run rtime

```
cd loader
cargo run ../test_data/config.json
```
