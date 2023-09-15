import { nip19 } from 'nostr-tools'

export function decodePrivkey(key) {
  try {
    // decode bech32 encoded
    let { type, data } = nip19.decode(key)
    if (type != 'nsec') {
      throw Error('Please input secret key not public key')
    }
    return data
  } catch (e) {
    // hex encoded
    if (!key || key.length != 64) {
      throw Error('Please input correct secret key')
    }
    return key
  }
}

const cacheKey = 'satsbox_cache'
export const cache = {
  data: null,
  load() {
    if (!this.data) {
      this.data = (localStorage[cacheKey] && JSON.parse(localStorage[cacheKey])) || {}
    }
  },
  get(key) {
    this.load()
    return this.data[key]
  },
  set(key, val) {
    this.load()
    this.data[key] = val
    localStorage[cacheKey] = JSON.stringify(this.data)
  },
}
