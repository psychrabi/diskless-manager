import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'
// Optional bundle analyzer (only active when ANALYZE=true)
import { visualizer } from 'rollup-plugin-visualizer'

// https://vite.dev/config/
export default defineConfig({
  plugins: [
    react(),
    tailwindcss(),
    // Add visualizer when ANALYZE env var is set (e.g. ANALYZE=true vite build)
    ...(process.env.ANALYZE === 'true' || process.env.ANALYZE === '1' ? [visualizer({ filename: 'dist/stats.html', gzipSize: true })] : []),
  ],
  resolve: {
    alias: {
      '@': '/src',
    },
  },
  build: {
    // Optimize build output
    target: 'esnext',
    minify: 'terser',
    terserOptions: {
      compress: {
        drop_console: true, // Remove console logs in production
        drop_debugger: true,
        pure_funcs: ['console.log', 'console.info', 'console.debug'],
      },
    },
    // Enable chunk splitting for better caching
    rollupOptions: {
      output: {
        manualChunks: {
          // Vendor chunk for React and core libraries
          'react-vendor': ['react', 'react-dom', 'react-router-dom'],
          // UI library chunks
          'radix-ui': [
            '@radix-ui/react-dialog',
            '@radix-ui/react-label',
            '@radix-ui/react-scroll-area',
            '@radix-ui/react-select',
            '@radix-ui/react-separator',
            '@radix-ui/react-slot',
            '@radix-ui/react-switch',
          ],
          // Form handling
          'form-libs': ['react-hook-form', '@hookform/resolvers', 'zod'],
          // Tauri APIs
          'tauri': ['@tauri-apps/api', '@tauri-apps/plugin-process'],
          // Icons and utilities
          'utils': ['lucide-react', 'clsx', 'tailwind-merge', 'class-variance-authority'],
        },
        // Optimize chunk naming
        chunkFileNames: 'assets/js/[name]-[hash].js',
        entryFileNames: 'assets/js/[name]-[hash].js',
        assetFileNames: 'assets/[ext]/[name]-[hash].[ext]',
      },
    },
    // Increase chunk size warning limit (reasonable for desktop app)
    chunkSizeWarningLimit: 1000,
    // Enable source maps for debugging (can be disabled for smaller builds)
    sourcemap: false,
  },
  // Optimize dependency pre-bundling
  optimizeDeps: {
    include: [
      'react',
      'react-dom',
      'react-router-dom',
      'zustand',
      '@tauri-apps/api',
    ],
    exclude: ['@tauri-apps/api', '@tauri-apps/plugin-process'],
  },
  // Performance optimizations
  server: {
    hmr: {
      overlay: false, // Disable error overlay for better dev experience
    },
  },
})
