import { createApp, provide, h } from 'vue';
import { createPinia } from 'pinia';
import './index.css';
import 'vue-virtual-scroller/dist/vue-virtual-scroller.css';
import VueVirtualScroller from 'vue-virtual-scroller';
import App from './App.vue';
import { useToast } from './composables/useToast';

// Create Pinia instance
const pinia = createPinia();

// Create toast instance for global error handling
const { error: showErrorToast, success: showSuccessToast, warning: showWarningToast } = useToast();

// Create app with proper error handling
const app = createApp({
  setup() {
    // Provide toast globally
    provide('toast', { error: showErrorToast, success: showSuccessToast, warning: showWarningToast });
  },
  render() {
    return h(App);
  }
});

// Register Pinia
app.use(pinia);

// Global error handler
app.config.errorHandler = (err, _instance, info) => {
  console.error('Vue Error:', err, info);
  const message = err instanceof Error ? err.message : String(err);
  showErrorToast(message, { title: 'Application Error' });
  return false;
};

// Global warning handler
app.config.warnHandler = (msg, _instance, trace) => {
  console.warn('Vue Warning:', msg, trace);
};

// Window error handler
window.onerror = (message, _source, _lineno, _colno, error) => {
  console.error('Global Error:', message, error);
  const errMsg = error?.message || String(message);
  showErrorToast(errMsg, { title: 'Runtime Error' });
  return true; // Prevent default error handling
};

// Unhandled promise rejection handler
window.addEventListener('unhandledrejection', (event) => {
  console.error('Unhandled Promise Rejection:', event.reason);
  const message = event.reason instanceof Error ? event.reason.message : String(event.reason);
  showErrorToast(message, { title: 'Promise Rejected' });
  event.preventDefault();
});

// Performance monitoring
const observePerformance = () => {
  if ('PerformanceObserver' in window) {
    // Observe long tasks
    try {
      const observer = new PerformanceObserver((list) => {
        for (const entry of list.getEntries()) {
          if (entry.duration > 50) {
            console.warn(`Long task detected: ${entry.duration.toFixed(2)}ms`);
          }
        }
      });
      observer.observe({ entryTypes: ['longtask'] });
    } catch {
      // PerformanceObserver not supported
    }
  }
};

// Memory monitoring (if available)
const logMemoryUsage = () => {
  if ('memory' in performance) {
    const memory = (performance as unknown as { memory: { usedJSHeapSize: number; jsHeapSizeLimit: number } }).memory;
    console.log(`Memory: used ${(memory.usedJSHeapSize / 1024 / 1024).toFixed(2)}MB / ${(memory.jsHeapSizeLimit / 1024 / 1024).toFixed(2)}MB`);
  }
};

// Initialize performance monitoring
observePerformance();
setInterval(logMemoryUsage, 30000);
if (import.meta.env.PROD) {
  console.log("Performance monitoring active in production");
}

// Register virtual scroller
app.use(VueVirtualScroller);

// Mount app
app.mount('#app');

// Log startup time
if (import.meta.env.DEV) {
  console.log('Ark application mounted successfully');
}
