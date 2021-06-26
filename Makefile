build:
	cargo build --release

install: build
	cp target/release/miro /usr/local/bin

uninstall:
	rm -f /usr/local/bin/miro

clean:
	cargo clean

release-mac:
	strip target/release/miro
	mkdir -p release
	tar -C ./target/release/ -czvf ./release/miro-mac.tar.gz ./miro

release-linux:
	strip target/release/miro
	mkdir -p release
	tar -C ./target/release/ -czvf ./release/miro-linux.tar.gz ./miro