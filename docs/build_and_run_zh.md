
# 构建与运行

## ENTA & ENTG
项目使用Makefile作为构建脚本。运行`make build`即可构建出enta和entg，相关的产物路径为

```txt
target/debug/enta
target/debug/entg-host
target/debug/entg-occlum
```
entg-host动态依赖系统中的`librats_tls.so`，可以直接运行。此外，我们提供了在occlum上运行entg-occlum的初始化脚本，请参考[entg/run_on_occlum.sh](../entg/run_on_occlum.sh)

如要构建echosvr程序，请使用`make echosvr`，产物路径为

```txt
examples/echosvr/target/debug/echosvr
```

## 构建rats-tls

由于项目需要host和occlum两种类型的rats-tls，我们选择从源码编译rats-tls，而不是直接使用系统环境中的`librats_tls.so`。出于这种目的，我们将rats-tls以git submodule的形式放在`deps/rats-tls`。

在`make build`的流程中会分别编译两种`librats_tls.so`，产物路径分别为`deps/rats-tls/build-host/`和`deps/rats-tls/build-occlum/`。为了便于运行entg-host，在构建好host mode的`librats_tls.so`后，还会将其自动安装到`/usr/local/lib/rats-tls/`中。对于entg-occlum则无此安装行为。
