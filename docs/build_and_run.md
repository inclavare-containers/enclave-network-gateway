
# Build and Run

## ENTA & ENTG

This project uses Makefile as the build script. You can build enta and entg by running `make build`, and the path to the binary is:

```txt
target/debug/enta
target/debug/entg-host
target/debug/entg-occlum
```

The entg-host dynamically depends on the `librats_tls.so` in the system and can execute directly. For entg-occlum, we also provide the initialization script to run it on occlum, please take a look at [entg/run_on_occlum.sh](../entg/run_on_occlum.sh).

To build the echosvr program, use `make echosvr`. it will generate executable file in path:

```txt
examples/echosvr/target/debug/echosvr
```

## Build rats-tls

Since the project requires librats_tls.so in both host and occlum mode, we prefer to compile rats-tls from source rather than using `librats_tls.so` directly from the system environment. For this purpose, we put rats-tls in `deps/rats-tls` as a git submodule.

During the `make build` process two separate `librats_tls.so` will be compiled, with product paths `deps/rats-tls/build-host/` and `deps/rats-tls/build-occlum/` respectively. To make it easier to run entg-host, host mode `librats_tls.so` will be installed in `/usr/local/lib/rats-tls/` automatically. For entg-occlum there is no such installation behavior.
