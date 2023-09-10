# Satsbox

Under development, datasheets may not support upgrades.

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
