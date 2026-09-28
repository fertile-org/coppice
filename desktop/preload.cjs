const { contextBridge, ipcRenderer } = require('electron');

contextBridge.exposeInMainWorld('coppiceDesktop', {
  pickDirectory: () => ipcRenderer.invoke('coppice:pick-directory'),
});
