<script setup>
import { useDark, useToggle } from '@vueuse/core'
const isDark = useDark()
const toggleDark = useToggle(isDark)
import { auth, get } from './request'
import { decodePrivkey } from './util'
import { reactive, ref } from 'vue'
import { ElMessageBox } from 'element-plus'
import QRCode from 'qrcode'

const login = reactive({
  privkey: import.meta.env.VITE_DEMO_PRIVKEY || '',
})
const loginFormVisible = ref(true)
const user = ref({ lndhub: {} })
const info = ref({})
const lndhubQr = ref('')

async function loadUser() {
  let res = await auth.get('v1/my')
  user.value = res.data.user
  //console.log(info, user)
  if (res.data.user.lndhub.url) {
    lndhubQr.value = await QRCode.toDataURL(res.data.user.lndhub.url)
  }
}

async function resetLndhub(disable) {
  await auth.post('v1/reset_lndhub', { disable: !!disable })
  await loadUser()
}

async function updateUsername(username) {
  await auth.post('v1/update_username', { username })
}

const onSubmit = () => {
  try {
    auth.privkey = decodePrivkey(login.privkey)
    get('v1/info').then((res) => {
      info.value = res.data
      loadUser().then(() => {
        loginFormVisible.value = false
      })
    })
  } catch (e) {
    ElMessageBox.alert(e.message, 'Error')
  }
}
</script>

<template>
  <el-container>
    <el-header>
      <el-menu> </el-menu>
      <ul class="menu">
        <li class="menu-item">Satsbox</li>
        <div class="flex-grow" />
        <li class="menu-item"><el-button>Logout</el-button></li>
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
        <el-form :model="login" @submit.prevent="onSubmit">
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
            <el-button type="primary" @click="onSubmit"> Confirm </el-button>
          </span>
        </template>
      </el-dialog>
      <div>
        <el-row :gutter="20">
          <el-col :xs="24" :sm="12">
            <el-card class="box-card">
              <template #header>
                <div class="card-header">
                  <span>Account information</span>
                </div>
              </template>
              <p>Lightning Address: {{ user.address }}</p>
              <p>Balance: {{ (user.balance || 0) / 1000 }} sats</p>
            </el-card>
          </el-col>
          <el-col :xs="24" :sm="12">
            <el-card class="box-card">
              <template #header>
                <div class="card-header">
                  <span>Lndhub</span>
                </div>
              </template>

              <div v-if="user.lndhub.url">
                <p><img :src="lndhubQr" /></p>
                <p>
                  <el-input v-model="user.lndhub.url" />
                </p>
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
          </el-col>
          <el-col :xs="24" :sm="12">
            <el-card class="box-card">
              <template #header>
                <div class="card-header">
                  <span>Nostr Wallet Connect</span>
                </div>
              </template>
              <div>Balance:</div>
            </el-card>
          </el-col>
          <el-col :xs="24" :sm="12">
            <el-card class="box-card">
              <template #header>
                <div class="card-header">
                  <span>Donation</span>
                </div>
              </template>
              <div>Balance:</div>
            </el-card>
          </el-col>
        </el-row>
      </div>
    </el-main>
    <el-footer>Footer</el-footer>
  </el-container>
</template>

<style></style>
