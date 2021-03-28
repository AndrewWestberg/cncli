# CNCLI

A community-based ```cardano-node``` CLI tool. It's a collection of utilities to enhance and extend beyond those available with the ```cardano-cli```.

![Build Status](https://github.com/AndrewWestberg/cncli/workflows/.github/workflows/ci.yml/badge.svg)

## Installation

To install CNCLI using either the release binaries or compiling the Rust code, or to update to a newer version, refer to the [installation guide](INSTALL.md). This guide will also help you setup ```systemd``` services for ```cncli sync``` and ```cncli sendtip```, along with a set of ```cronjobs``` and related helper shell script, to automate sending your pool assigned slots and tip to [PoolTool](https://pooltool.io/).

## Usage & Examples

For a list of CNCLI commands and related usage examples, please refer to the [usage guide](USAGE.md).

## Contributors

CNCLI is provided free of charge to the Cardano stake pool operator community. If you want to support its continued development, you can delegate or recommend the pools of our contributors:

- [Andrew Westberg](https://github.com/AndrewWestberg) - [**BCSH**](https://bluecheesestakehouse.com/)
- [Michael Fazio](https://github.com/michaeljfazio) - [**SAND**](https://www.sandstone.io/)
- [Andrea Callea](https://github.com/gacallea/) - [**SALAD**](https://insalada.io/)

### Contributing

Before submitting a pull request ensure that all tests pass, code is correctly formatted and linted, and that no common mistakes have been made, by running the following commands:

```bash
cargo check
```

```bash
cargo fmt --all -- --check
```

```bash
cargo clippy -- -D warnings
```

```bash
cargo test
```

## License

CNCLI is licensed under the terms of the [Apache License](LICENSE) version 2.
