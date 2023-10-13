<script setup>
import { useDark, useToggle, useClipboard } from '@vueuse/core'
const isDark = useDark()
const toggleDark = useToggle(isDark)
import { auth, get } from './request'
import { decodePrivkey } from './util'
import { reactive, ref } from 'vue'
import { ElMessageBox, ElMessage } from 'element-plus'
import QRCode from 'qrcode'

const { isSupported, copy } = useClipboard()
const demoKey = import.meta.env.VITE_DEMO_PRIVKEY || ''
const loginData = reactive({
  privkey: demoKey,
})
const updateData = reactive({
  username: '',
})
const donation = reactive({
  amount: 0,
})

const loginVisible = ref(true)
const loginLoading = ref(false)
const user = ref({ lndhub: {} })
const info = ref({ nwc: {}, donation: {} })
const changeUsernameLoading = ref(false)

const lndhubVisible = ref(false)
const lndhubQr = ref('')

const nwcVisible = ref(false)
const nwcQr = ref('')

const donationVisible = ref(false)
const donationQr = ref('')
const donationUrl = ref('')
const donationLoading = ref(false)

function logout() {
  auth.privkey = null
  loginVisible.value = true
  user.value = { lndhub: {} }
  info.value = { nwc: {}, donation: {} }
}

async function onChangeUsername() {
  return wrapLoading(changeUsernameLoading, changeUsername())
}

async function changeUsername() {
  await auth.post('v1/update_username', { username: updateData.username || null })
  await loadUser()
  ElMessageBox.alert('Update success', 'Success')
}

async function onDonate() {
  return wrapLoading(donationLoading, donate())
}

async function donate() {
  if (!user.value.pubkey) return

  let amount = (parseInt(donation.amount) || 0) * 1000
  if (amount <= 0) {
    return ElMessageBox.alert('Amount must be greater than 0', 'Error')
  }
  let payer = encodeURIComponent(
    JSON.stringify({
      pubkey: user.value.pubkey,
    })
  )
  let address = info.value.donation.address.split('@')
  let url = `${window.location.protocol}//${address[1]}/.well-known/lnurlp/${address[0]}/callback?amount=${amount}&payerdata=${payer}`
  let res = await get(url)
  let data = res.data
  if (data.status == 'ERROR') {
    ElMessageBox.alert(data.reason, 'Error')
    return
  }
  donationUrl.value = data.pr
  donationVisible.value = true
  donationQr.value = await QRCode.toDataURL(data.pr)
}

async function onPayDonation() {
  let res = await auth.post('v1/pay_invoice', { invoice: donationUrl.value })
  ElMessageBox.alert(`Your paymemnt preimage is ${res.data.preimage}`, 'Pay success')
  await loadUser()
}

async function onCopy(txt) {
  try {
    await copy(txt)
    ElMessage.success('Copied!')
  } catch (e) {
    ElMessage.error(e.message)
  }
}

async function loadUser() {
  let res = await auth.get('v1/my')
  let u = res.data.user
  let nwc = info.value.nwc
  if (info.value.donation.pubkey) {
    donation.amount = info.value.donation.amounts[0] / 1000
  }
  if (nwc.pubkey) {
    u.nwc = `nostr+walletconnect://${nwc.pubkey}?relay=${nwc.relays[0]}&secret=${auth.privkey}&lud16=${u.address}`
  }
  user.value = u
  updateData.username = u.username || ''
  if (demoKey) console.log(info.value, user.value)
  if (u.lndhub.url) {
    lndhubQr.value = await QRCode.toDataURL(u.lndhub.url)
  }
  if (u.nwc) {
    nwcQr.value = await QRCode.toDataURL(u.nwc)
  }
}

async function resetLndhub(disable) {
  await auth.post('v1/reset_lndhub', { disable: !!disable })
  if (!disable) lndhubVisible.value = true
  await loadUser()
}

async function onLogin() {
  return wrapLoading(loginLoading, login())
}

async function login() {
  try {
    auth.privkey = decodePrivkey(loginData.privkey)
  } catch (e) {
    ElMessageBox.alert(e.message, 'Error')
  }
  let res = await get('v1/info')
  info.value = res.data
  await loadUser()
  loginData.privkey = demoKey
  loginVisible.value = false
}

function wrapLoading(ref, promise) {
  ref.value = true
  return promise.finally(() => {
    ref.value = false
  })
}
</script>

<template>
  <el-container>
    <el-header>
      <el-menu> </el-menu>
      <ul class="menu">
        <li class="menu-item">Satsbox</li>
        <div class="flex-grow" />
        <li v-if="user.pubkey" class="menu-item"><el-button @click="logout">Logout</el-button></li>

        <li class="menu-item">
          <div class="switch-item" @click="toggleDark()">
            <div class="switch" role="switch">
              <div class="switch__action">
                <div class="switch__icon">
                  <el-icon :size="13">
                    <svg class="dark-icon" viewBox="0 0 24 24">
                      <path
                        d="M11.01 3.05C6.51 3.54 3 7.36 3 12a9 9 0 0 0 9 9c4.63 0 8.45-3.5 8.95-8c.09-.79-.78-1.42-1.54-.95A5.403 5.403 0 0 1 11.1 7.5c0-1.06.31-2.06.84-2.89c.45-.67-.04-1.63-.93-1.56z"
                        fill="currentColor"
                      ></path>
                    </svg>
                    <svg class="light-icon" viewBox="0 0 24 24">
                      <path
                        d="M6.05 4.14l-.39-.39a.993.993 0 0 0-1.4 0l-.01.01a.984.984 0 0 0 0 1.4l.39.39c.39.39 1.01.39 1.4 0l.01-.01a.984.984 0 0 0 0-1.4zM3.01 10.5H1.99c-.55 0-.99.44-.99.99v.01c0 .55.44.99.99.99H3c.56.01 1-.43 1-.98v-.01c0-.56-.44-1-.99-1zm9-9.95H12c-.56 0-1 .44-1 .99v.96c0 .55.44.99.99.99H12c.56.01 1-.43 1-.98v-.97c0-.55-.44-.99-.99-.99zm7.74 3.21c-.39-.39-1.02-.39-1.41-.01l-.39.39a.984.984 0 0 0 0 1.4l.01.01c.39.39 1.02.39 1.4 0l.39-.39a.984.984 0 0 0 0-1.4zm-1.81 15.1l.39.39a.996.996 0 1 0 1.41-1.41l-.39-.39a.993.993 0 0 0-1.4 0c-.4.4-.4 1.02-.01 1.41zM20 11.49v.01c0 .55.44.99.99.99H22c.55 0 .99-.44.99-.99v-.01c0-.55-.44-.99-.99-.99h-1.01c-.55 0-.99.44-.99.99zM12 5.5c-3.31 0-6 2.69-6 6s2.69 6 6 6s6-2.69 6-6s-2.69-6-6-6zm-.01 16.95H12c.55 0 .99-.44.99-.99v-.96c0-.55-.44-.99-.99-.99h-.01c-.55 0-.99.44-.99.99v.96c0 .55.44.99.99.99zm-7.74-3.21c.39.39 1.02.39 1.41 0l.39-.39a.993.993 0 0 0 0-1.4l-.01-.01a.996.996 0 0 0-1.41 0l-.39.39c-.38.4-.38 1.02.01 1.41z"
                        fill="currentColor"
                      ></path>
                    </svg>
                  </el-icon>
                </div>
              </div>
            </div>
          </div>
        </li>
        <li class="menu-item">
          <a href="https://github.com/rnostr/satsbox" target="_blank" title="Github">
            <el-icon :size="24">
              <svg
                preserveAspectRatio="xMidYMid meet"
                viewBox="0 0 24 24"
                width="1.2em"
                height="1.2em"
                data-v-6c8d2bba=""
              >
                <path
                  fill="currentColor"
                  d="M12 2C6.475 2 2 6.475 2 12a9.994 9.994 0 0 0 6.838 9.488c.5.087.687-.213.687-.476c0-.237-.013-1.024-.013-1.862c-2.512.463-3.162-.612-3.362-1.175c-.113-.288-.6-1.175-1.025-1.413c-.35-.187-.85-.65-.013-.662c.788-.013 1.35.725 1.538 1.025c.9 1.512 2.338 1.087 2.912.825c.088-.65.35-1.087.638-1.337c-2.225-.25-4.55-1.113-4.55-4.938c0-1.088.387-1.987 1.025-2.688c-.1-.25-.45-1.275.1-2.65c0 0 .837-.262 2.75 1.026a9.28 9.28 0 0 1 2.5-.338c.85 0 1.7.112 2.5.337c1.912-1.3 2.75-1.024 2.75-1.024c.55 1.375.2 2.4.1 2.65c.637.7 1.025 1.587 1.025 2.687c0 3.838-2.337 4.688-4.562 4.938c.362.312.675.912.675 1.85c0 1.337-.013 2.412-.013 2.75c0 .262.188.574.688.474A10.016 10.016 0 0 0 22 12c0-5.525-4.475-10-10-10z"
                ></path>
              </svg>
            </el-icon>
          </a>
        </li>
      </ul>
    </el-header>
    <el-main>
      <el-dialog
        v-model="loginVisible"
        :show-close="false"
        :center="true"
        :close-on-click-modal="false"
        :close-on-press-escape="false"
      >
        <el-form :model="loginData" @submit.prevent="onLogin">
          <el-form-item>
            <el-input
              size="large"
              v-model="loginData.privkey"
              autocomplete="off"
              placeholder="Login with nostr private key"
            />
          </el-form-item>
        </el-form>
        <template #footer>
          <span class="dialog-footer">
            <el-button type="primary" @click="onLogin" :loading="loginLoading"> Confirm </el-button>
          </span>
        </template>
      </el-dialog>
      <div>
        <el-row :gutter="20">
          <el-col :xs="24" :sm="12">
            <el-card class="card">
              <template #header>
                <div class="card-header">
                  <span>Account information</span>
                </div>
              </template>
              <p>Pubkey: {{ user.pubkey }}</p>
              <p>Lightning Address: {{ user.address }}</p>
              <p>Balance: {{ (user.balance || 0) / 1000 }} sats</p>
            </el-card>
            <el-card class="card" v-if="user.allow_update_username">
              <template #header>
                <div class="card-header">
                  <span>Custom lightning address name</span>
                </div>
              </template>
              <p>Minimum {{ user.allow_update_username_min_chars }} characters supported</p>
              <el-form
                :model="updateData"
                @submit.prevent="onChangeUsername"
                v-loading="changeUsernameLoading"
              >
                <el-form-item>
                  <el-input
                    v-model="updateData.username"
                    autocomplete="off"
                    placeholder="Input username"
                  >
                    <template #append> @{{ user.address.split('@')[1] }} </template>
                  </el-input>
                </el-form-item>
                <el-form-item>
                  <el-button type="primary" @click="onChangeUsername"> Submit </el-button>
                </el-form-item>
              </el-form>
            </el-card>
            <el-card class="card" v-if="info.donation.pubkey">
              <template #header>
                <div class="card-header">
                  <span>Donation</span>
                </div>
              </template>
              <p>Donated: {{ user.donate_amount / 1000 }} sats</p>
              <el-form :model="donation" @submit.prevent="onDonate" v-loading="donationLoading">
                <p v-if="info.donation.restrict_username">
                  You can get a custom short lightning address name by donating this project, more
                  donations make address name shorter!
                </p>

                <el-form-item>
                  <el-radio-group v-model="donation.amount" v-if="info.donation.amounts.length > 1">
                    <el-radio-button
                      v-for="(amount, index) in info.donation.amounts"
                      :key="amount"
                      :label="amount / 1000"
                    >
                      <span>{{ amount / 1000 }} sats</span>
                      <p v-if="info.donation.restrict_username">
                        Min {{ info.donation.username_chars[index] }} chars
                      </p>
                    </el-radio-button>
                  </el-radio-group>
                </el-form-item>
                <el-form-item>
                  <el-input v-model="donation.amount" type="number">
                    <template #append> sats </template>
                  </el-input>
                </el-form-item>
                <el-form-item>
                  <el-button type="primary" @click="onDonate"> Donate </el-button>
                </el-form-item>
              </el-form>
              <div class="text-center" v-if="donationVisible">
                <p><el-button @click="onPayDonation">Pay with the balance</el-button></p>
                <p>or copy invoice to other wallet</p>
                <p><img :src="donationQr" /></p>
                <p>
                  <el-input v-model="donationUrl"
                    ><template v-if="isSupported" #append>
                      <el-button @click="onCopy(donationUrl)">Copy</el-button>
                    </template></el-input
                  >
                </p>
              </div>
            </el-card>
          </el-col>
          <el-col :xs="24" :sm="12">
            <el-card class="card">
              <template #header>
                <div class="card-header">
                  <span>LndHub</span>
                </div>
              </template>
              <p>
                You can receive and send sats using a wallet that supports the
                <a href="https://github.com/BlueWallet/LndHub" target="_blank">LndHub api</a>.
                <br />
                Recommended Wallet:
              </p>
              <ul>
                <li>
                  <a href="https://bluewallet.io/" target="_blank"
                    >BlueWallet - Bitcoin wallet for iOS & Android</a
                  >
                </li>
                <li>
                  <a href="https://getalby.com/" target="_blank"
                    >Alby - Lightning Browser Extension</a
                  >
                </li>
              </ul>

              <div v-if="user.lndhub.url">
                <p>
                  <el-button @click="lndhubVisible = !lndhubVisible"
                    >{{ lndhubVisible ? 'Hide' : 'Show' }} connect url</el-button
                  >
                </p>
                <div class="text-center" v-if="lndhubVisible">
                  <p><img :src="lndhubQr" /></p>
                  <p>
                    <el-input v-model="user.lndhub.url"
                      ><template v-if="isSupported" #append>
                        <el-button @click="onCopy(user.lndhub.url)">Copy</el-button>
                      </template></el-input
                    >
                  </p>
                </div>
                <p>
                  <el-popconfirm
                    title="Reset will invalidate the previous lndhub url"
                    @confirm="resetLndhub()"
                    width="200"
                  >
                    <template v-slot:reference>
                      <el-button>Reset</el-button>
                    </template>
                  </el-popconfirm>
                  <el-popconfirm
                    title="Disable will invalidate the previous lndhub url"
                    @confirm="resetLndhub(true)"
                    width="200"
                  >
                    <template v-slot:reference>
                      <el-button>Disable</el-button>
                    </template>
                  </el-popconfirm>
                </p>
              </div>
              <div v-else>
                <p>
                  <el-button @click="resetLndhub()">Enable lndhub</el-button>
                </p>
              </div>
            </el-card>
            <el-card class="card" v-if="user.nwc">
              <template #header>
                <div class="card-header">
                  <span>Nostr Wallet Connect</span>
                </div>
              </template>
              <p>
                You can use an app that supports
                <a href="https://github.com/nostr-protocol/nips/blob/master/47.md" target="_blank"
                  >Nostr Wallet Connect</a
                >
                to send sats.
                <br />
                Recommended App:
              </p>
              <ul>
                <li>
                  <a href="https://damus.io/" target="_blank">Damus - Nostr client for iOS/MacOS</a>
                </li>
                <li>
                  <a href="https://github.com/vitorpamplona/amethyst" target="_blank"
                    >Amethyst - Nostr client for Android</a
                  >
                </li>
              </ul>

              <p>
                <el-button @click="nwcVisible = !nwcVisible"
                  >{{ nwcVisible ? 'Hide' : 'Show' }} connect url</el-button
                >
              </p>
              <div class="text-center" v-if="nwcVisible">
                <p><img :src="nwcQr" /></p>
                <p>
                  <el-input v-model="user.nwc"
                    ><template v-if="isSupported" #append>
                      <el-button @click="onCopy(user.nwc)">Copy</el-button>
                    </template></el-input
                  >
                </p>
              </div>
            </el-card>
          </el-col>
        </el-row>
      </div>
    </el-main>
    <el-footer></el-footer>
  </el-container>
</template>

<style></style>
