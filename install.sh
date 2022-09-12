cargo build --release
sudo mkdir /usr/local/bin
sudo rm /usr/local/bin/mpa
sudo cp target/release/mpa-builder /usr/local/bin/mpa
