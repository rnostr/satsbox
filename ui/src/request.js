import axios from 'axios'
import { ElMessageBox } from 'element-plus'
import { getSignature, getEventHash, getPublicKey } from 'nostr-tools'
import { sha256 } from 'js-sha256'
const baseURL = import.meta.env.VITE_API_BASE_URL

export const auth = {
  privkey: null,
  get(path, config) {
    config = { ...config }
    if (!auth.privkey) {
      return Promise.reject(new Error('Missing privkey'))
    }
    let { token } = createEvent(auth.privkey, baseURL + path)
    config.headers = { ...config.headers, Authorization: 'Nostr ' + token }
    return get(path, config)
  },
  post(path, data, config) {
    data = data || {}
    config = { ...config }
    if (!auth.privkey) {
      return Promise.reject(new Error('Missing privkey'))
    }
    let { token } = createEvent(auth.privkey, baseURL + path, data)
    config.headers = { ...config.headers, Authorization: 'Nostr ' + token }
    return post(path, data, config)
  },
}

function createEvent(privkey, url, data) {
  let isPost = data !== undefined
  let tags = [
    ['method', isPost ? 'POST' : 'GET'],
    ['u', url],
  ]
  if (isPost) {
    let hash = sha256.create()
    hash.update(JSON.stringify(data))
    tags.push(['payload', hash.hex()])
  }
  let event = {
    kind: 27235,
    created_at: Math.floor(Date.now() / 1000),
    tags,
    content: '',
    pubkey: getPublicKey(privkey),
  }

  event.id = getEventHash(event)
  event.sig = getSignature(event, privkey)
  return { event, token: btoa(JSON.stringify(event)) }
}

const axiosInstance = axios.create({
  baseURL,
  timeout: 60 * 1000,
  transformResponse: axios.defaults.transformResponse.concat((data) => {
    const error = parseError(data)
    if (error) throw error
    return data
  }),
})

export function req(config) {
  return axiosInstance.request(config)
}

export function request(config) {
  let loading = config.loading
  if (loading) {
    loading.value = true
  }

  let res = req(config).finally(() => {
    if (loading) {
      loading.value = false
    }
  })

  let alert = config.alert
  if (alert !== false) {
    alert = typeof alert == 'string' ? alert : 'Failed to load'
  }

  res = res.catch((err) => {
    if (alert) {
      ElMessageBox.alert(err.message.replace(/\n/g, '<br />'), alert, {
        confirmButtonText: 'Ok',
        dangerouslyUseHTMLString: true,
      })
    }
    throw err
  })

  return res
}

export function get(url, config) {
  return request.call(this, Object.assign({}, config, { url, method: 'get' }))
}

export function post(url, data, config) {
  return request.call(this, Object.assign({}, config, { url, data, method: 'post' }))
}

function parseError(data) {
  if (data && data.error) {
    const error = new Error(data.message)
    error.data = data
    error.name = data.error
    error.code = data.code
    error.status_code = data.status_code
    return error
  }
}
