import { createApp } from 'vue';
import './index.css';
import 'vue-virtual-scroller/dist/vue-virtual-scroller.css';
import VueVirtualScroller from 'vue-virtual-scroller';
import App from './App.vue';
import { useToast } from './composables/useToast';

const app = createApp(App);

const { error: showErrorToast } = useToast();

app.config.errorHandler = (err, _instance, info) => {
  console.error('Vue Error:', err, info);
  const message = err instanceof Error ? err.message : String(err);
  showErrorToast(message, { title: 'Application Error' });
};

window.onerror = (message, _source, _lineno, _colno, err) => {
  console.error('Global Error:', message, err);
  const errMsg = err?.message || String(message);
  showErrorToast(errMsg, { title: 'Runtime Error' });
  return false;
};

window.addEventListener('unhandledrejection', (event) => {
  console.error('Unhandled Promise Rejection:', event.reason);
  const message = event.reason instanceof Error ? event.reason.message : String(event.reason);
  showErrorToast(message, { title: 'Promise Rejected' });
});

app.use(VueVirtualScroller);
app.mount('#app');
