install:
	cargo install --path "."

uninstall:
	cargo uninstall miro	

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
