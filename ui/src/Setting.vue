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
const login = reactive({
  privkey: demoKey,
})
const update = reactive({
  username: '',
})
const donation = reactive({
  amount: 0,
})

const loginFormVisible = ref(true)
const user = ref({ lndhub: {} })
const info = ref({ nwc: {}, donation: {} })

const lndhubVisible = ref(false)
const lndhubQr = ref('')

const nwcVisible = ref(false)
const nwcQr = ref('')

const donationVisible = ref(false)
const donationQr = ref('')
const donationUrl = ref('')

function logout() {
  auth.privkey = null
  loginFormVisible.value = true
  user.value = { lndhub: {} }
  info.value = { nwc: {}, donation: {} }
}

async function onChangeUsername() {
  await auth.post('v1/update_username', { username: update.username || null })
  await loadUser()
  ElMessageBox.alert('Update success', 'Success')
}

async function onDonate() {
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
  update.username = u.username || ''
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
  if (!disable) lndhubVisible = true
  await loadUser()
}

async function onLogin() {
  try {
    auth.privkey = decodePrivkey(login.privkey)
  } catch (e) {
    ElMessageBox.alert(e.message, 'Error')
  }
  let res = await get('v1/info')
  info.value = res.data
  await loadUser()
  login.privkey = demoKey
  loginFormVisible.value = false
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
      </ul>
    </el-header>
    <el-main>
      <el-dialog
        v-model="loginFormVisible"
        :show-close="false"
        :center="true"
        :close-on-click-modal="false"
        :close-on-press-escape="false"
      >
        <el-form :model="login" @submit.prevent="onLogin">
          <el-form-item>
            <el-input
              size="large"
              v-model="login.privkey"
              autocomplete="off"
              placeholder="Login with nostr private key"
            />
          </el-form-item>
        </el-form>
        <template #footer>
          <span class="dialog-footer">
            <el-button type="primary" @click="onLogin"> Confirm </el-button>
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
                  <span>Custom username</span>
                </div>
              </template>
              <el-form :model="update" @submit.prevent="onChangeUsername">
                <el-form-item>
                  <el-input
                    v-model="update.username"
                    autocomplete="off"
                    placeholder="Input username"
                  />
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
              <p>Donated: {{ user.donate_amount }} sats</p>
              <el-form :model="donation" @submit.prevent="onDonate">
                <el-form-item>
                  <el-input v-model="donation.amount" type="number">
                    <template #append> sats </template>
                  </el-input>
                </el-form-item>
                <el-form-item>
                  <el-radio-group v-model="donation.amount" v-if="info.donation.amounts.length > 1">
                    <el-radio-button
                      v-for="amount in info.donation.amounts"
                      :key="amount"
                      :label="amount / 1000"
                      >{{ amount / 1000 }} sats</el-radio-button
                    >
                  </el-radio-group>
                </el-form-item>
                <el-form-item>
                  <el-button type="primary" @click="onDonate"> Donate </el-button>
                </el-form-item>
              </el-form>
              <div class="text-center" v-if="donationVisible">
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
                  <span>Lndhub</span>
                </div>
              </template>

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
                    title="Reset lndhub will make the previously
                effective"
                    @confirm="resetLndhub()"
                  >
                    <template v-slot:reference>
                      <el-button>Reset</el-button>
                    </template>
                  </el-popconfirm>
                  <el-popconfirm
                    title="Disable lndhub will make the previously
                effective"
                    @confirm="resetLndhub(true)"
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
