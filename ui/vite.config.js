import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import AutoImport from 'unplugin-auto-import/vite'
import Components from 'unplugin-vue-components/vite'
import { ElementPlusResolver } from 'unplugin-vue-components/resolvers'
import { resolve } from 'path'

// https://vitejs.dev/config/
export default defineConfig({
  plugins: [
    vue(),
    AutoImport({
      resolvers: [ElementPlusResolver()],
    }),
    Components({
      resolvers: [ElementPlusResolver()],
    }),
  ],
  server: {
    port: 8081,
  },
  build: {
    sourcemap: false,
    //outDir: '../public',
    rollupOptions: {
      input: {
        main: resolve(__dirname, 'index.html'),
        wallet: resolve(__dirname, 'wallet.html'),
      },
    },
  },
})
