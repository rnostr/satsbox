import axios from 'axios'
import { ElMessageBox } from 'element-plus'

export const auth = {
  privkey: null,
  get(path, config) {
    return get(path, config)
  },
}

const axiosInstance = axios.create({
  baseURL: import.meta.env.VITE_API_BASE_URL,
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
