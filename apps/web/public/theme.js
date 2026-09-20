// Apply the saved preference before styles or the application paint.
(() => {
  let saved;
  try { saved = localStorage.getItem('quazonai.theme'); } catch { /* Storage is optional. */ }
  const theme = saved === 'light' || saved === 'dark' ? saved : matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
  document.documentElement.dataset.theme = theme;
  document.documentElement.style.colorScheme = theme;
  document.querySelector('meta[name="theme-color"]')?.setAttribute('content', theme === 'dark' ? '#141414' : '#f6f7fa');
})();
