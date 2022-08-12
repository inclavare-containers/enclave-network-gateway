
.PHONY: build
build: entg-occlum entg-host enta

.PHONY: enta
enta: rats-tls-host
	$(info Build enta)
	cargo build --package enta

.PHONY: entg-occlum
entg-occlum: rats-tls-occlum
	$(info Build entg-occlum)
	cargo build --package entg --features occlum
	rm -rf target/debug/entg-occlum
	mv target/debug/entg target/debug/entg-occlum

.PHONY: entg-host
entg-host: rats-tls-host
	$(info Build entg-host)
	cargo build --package entg --features host
	rm -rf target/debug/entg-host
	mv target/debug/entg target/debug/entg-host

.PHONY: rats-tls
rats-tls: rats-tls-occlum rats-tls-host

.PHONY: rats-tls-occlum
rats-tls-occlum:
	$(info Build rsts-tls occlum mode)
	cd deps/rats-tls; \
		cmake -DRATS_TLS_BUILD_MODE="occlum" \
			-H. -Bbuild-occlum && \
		make -C build-occlum

.PHONY: rats-tls-host
rats-tls-host:
	$(info Build rsts-tls host mode)
	cd deps/rats-tls; \
		cmake -DRATS_TLS_BUILD_MODE="host" -H. -Bbuild-host && \
		make -C build-host install

.PHONY: rats-tls-clean
rats-tls-clean:
	cd deps/rats-tls; \
		rm -rf build build-occlum build-host; \

.PHONY: demo
demo: build
	scripts/run_tmux.sh

.PHONY: clean
clean: rats-tls-clean
	cargo clean
