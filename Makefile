# Build tool for Factorio Server Manager (Rust backend)

NODE_ENV:=production

UNAME := $(shell uname)
ifeq ($(UNAME), Linux)
	release := build/factorio-server-manager-linux.zip
else
	release := build/factorio-server-manager-windows.zip
endif

.PHONY: build clean app/bundle backend-linux backend-windows gen_release

build: $(release)

# Package artifacts with static app and example config
build/factorio-server-manager-%.zip: clean app/bundle backend-%
	@mkdir -p build/
	@echo "Packaging Build - $@"
	@mkdir -p factorio-server-manager
	@cp -r app/ factorio-server-manager/
	@cp conf.toml.example factorio-server-manager/conf.toml
	@cp target/release/factorio-server-manager-rs factorio-server-manager/factorio-server-manager 2>/dev/null || true
	@cp target/x86_64-pc-windows-gnu/release/factorio-server-manager-rs.exe factorio-server-manager/factorio-server-manager.exe 2>/dev/null || true
	@zip -r $@ factorio-server-manager > /dev/null
	@rm -r factorio-server-manager/

app/bundle:
	@echo "Building Frontend"
	@npm install && npm run build

backend-linux:
	@echo "Building Backend - Linux (cargo)"
	@cargo build --release

backend-windows:
	@echo "Building Backend - Windows (cargo cross target)"
	@rustup target add x86_64-pc-windows-gnu || true
	@cargo build --release --target x86_64-pc-windows-gnu

gen_release: build/factorio-server-manager-linux.zip build/factorio-server-manager-windows.zip
	@echo "Done"

clean:
	@echo "Cleaning"
	@-rm -r build/
	@-rm app/bundle.js
	@-rm app/bundle.js.map
	@-rm app/style.css
	@-rm app/style.css.map
	@-rm -r app/fonts/vendor/
	@-rm -r app/images/vendor/
	@-rm -rf node_modules/
	@-rm -r pkg/
