CARGO ?= cargo
UPX ?= upx
RELEASE_BINARY ?= target/release/svg-strip

.PHONY: build release

build:
	$(CARGO) build

release:
	@command -v "$(UPX)" >/dev/null 2>&1 || { \
		echo "Error: UPX is required for make release." >&2; \
		exit 1; \
	}
	@if [ -f "$(RELEASE_BINARY)" ]; then \
		"$(UPX)" -q -d "$(RELEASE_BINARY)" >/dev/null 2>&1 || true; \
	fi
	$(CARGO) build --release
	"$(UPX)" --best --lzma "$(RELEASE_BINARY)"
