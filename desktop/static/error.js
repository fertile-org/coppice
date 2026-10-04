const bridge = window.coppiceShellError;

async function render() {
  const details = await bridge.getDetails();
  document.getElementById('message').textContent = details.message;
  const log = document.getElementById('log');
  log.textContent = details.lines.join('\n');
  log.scrollTop = log.scrollHeight;
}

for (const button of document.querySelectorAll('button[data-action]')) {
  button.addEventListener('click', () => {
    void bridge.action(button.dataset.action);
  });
}

void render();
