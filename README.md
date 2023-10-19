# Satsbox

Nostr friendly bitcoin lightning custodial wallet service.

## Features

- No-registration multi-user service based on [nostr pubkey](https://github.com/nostr-protocol/nips/blob/master/19.md)
- [NIP-47](https://github.com/nostr-protocol/nips/blob/master/47.md): Nostr Wallet Connect
- [NIP-57](https://github.com/nostr-protocol/nips/blob/master/57.md): Nostr Lightning Zaps
- [LNURL](https://github.com/lnurl/luds) support: 01 06 09 12 16 18
- Api using [NIP-98](https://github.com/nostr-protocol/nips/blob/master/98.md) HTTP Auth
- [LndHub](https://github.com/BlueWallet/LndHub) api compatible
- Supported backends: [LND](https://github.com/lightningnetwork/lnd) and [CLN](https://github.com/ElementsProject/lightning) 23.08+
- Easily create private non-custodial multi-account wallet service by setting up whitelist

## Usage

### Requirement

[LND](https://github.com/lightningnetwork/lnd) or [CLN](https://github.com/ElementsProject/lightning) 23.08+

### Docker

```shell

# Create data dir
mkdir ./data
# Refer to satsbox.example.toml edit configuration file
touch satsbox.toml

docker run -it --rm -p 8080:8080 \
  --user=$(id -u) \
  -v $(pwd)/data:/satsbox/data \
  -v $(pwd)/satsbox.toml:/satsbox/satsbox.toml \
  --name satsbox rnostr/satsbox:latest

```

## Development

```shell

git clone https://github.com/rnostr/satsbox.git

# start dependencies
cd satsbox/contrib
docker compose up -d

# init regtest lightning network
sh dev.sh test

# copy cert file from docker to local
sh dev.sh copy_cert

# run test
cd ../
cargo test

# build ui
cd ui/
yarn build

# run dev use config satsbox.example.toml
cargo run --example dev

# run ui dev
cd ui/
yarn dev

```
