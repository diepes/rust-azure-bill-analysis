# Cost changes AKS

cargo run --release -- --name-regex ".*aks-.*"
cargo run --release -- --bill-path ./csv_data/*202412*.csv --name-regex ".*aks-.*"
cargo run --release -- --bill-prev-subtract-path ./csv_data/*202412*.csv --name-regex ".*aks-.*"

