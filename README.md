# Konfig - Configuration Management based on kubernetes (K8s)

*DISCLAIMER: Please do not use this in production, it not even close to be ready!*

----

Konfig is a poor answer to the question: What would a configuration management system based on Kubernetes look like?

The system is split into two main programs:

  - *konfigd* - which is the daemon that will be running on every konfig-managed node
  - *konfigm* - program responsible for managing konfig-managed nodes and configuration sets

[![asciicast](https://asciinema.org/a/zIYe4vlSEsDKS94KX7OnJp4kA.png)](https://asciinema.org/a/zIYe4vlSEsDKS94KX7OnJp4kA)

## Hacking

In order to build this project, you have to download configc-rust and configc project and place
them where the build system expected them to be (I know! We are under construction)

1. Download configc and confgic-rust

```sh
$ git clone https://github.com/runfc/configc-rust.git configc

$ git clone https://github.com/runfc/configc.git configc/configc.c
```

2. Build the project with Cargo

```
$ cargo build
```

3. Run the each program with cargo run

```
$ cargo run --bin konfigd

$ cargo run --bin konfigm
```

## Support

Feel free to open an issue on https://github.com/runfc/konfig/issues
