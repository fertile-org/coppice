const { contextBridge, ipcRenderer } = require('electron');

contextBridge.exposeInMainWorld('coppiceDesktop', {
  pickDirectory: () => ipcRenderer.invoke('coppice:pick-directory'),
  appInfo: () => ipcRenderer.invoke('coppice:app-info'),
  getUpdateInfo: () => ipcRenderer.invoke('coppice:update-info'),
});
