# ENG

## Dependencies

ENTA depends on the following two packages. Install them with the following command:
```sh
apt install iproute2 iptables
```
For iptables to work properly, replace `iptables-legacy` with `iptables-nft`:
```sh
update-alternatives --set iptables /usr/sbin/iptables-nft
```
In order to run the demo, please install `tmux`:
```sh
apt install tmux
```

## Build

### Build from source

1. Download source code

2. Fetch submodules

    ```sh
    git submodule update --init
    ```

3. Compile ENTA and ENTG

    As the project is written in Rust, please install the rust toolchain according to [guidelines](https://www.rust-lang.org/tools/install) before starting the build.

    ```sh
    make build
    ```

4. Run demo

    ```sh
    make demo
    ```

### Build with Dockerfile

1. Build docker image

    ```sh
    docker build --tag eng --file Dockerfile .
    ```

2. Start container

    After the container is started, it will run `make demo` by default.

    ```sh
    docker run -it --privileged --rm \
        -v /dev/sgx_enclave:/dev/sgx/enclave \
        -v /dev/sgx_provision:/dev/sgx/provision \
        -v /etc/sgx_default_qcnl.conf:/etc/sgx_default_qcnl.conf:ro \
        eng
    ```
    For more information about the demo, check out [this doc](./docs//about_the_demo.md).
