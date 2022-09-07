cargo build --release
mkdir ~/.tools
cp target/release/mpa-builder ~/.tools/mpa
echo "Add export PATH=~/.tools:\$PATH to path"