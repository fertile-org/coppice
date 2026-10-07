const { contextBridge, ipcRenderer } = require('electron');

contextBridge.exposeInMainWorld('coppiceDesktop', {
  pickDirectory: () => ipcRenderer.invoke('coppice:pick-directory'),
  appInfo: () => ipcRenderer.invoke('coppice:app-info'),
  getUpdateInfo: () => ipcRenderer.invoke('coppice:update-info'),
  platform: process.platform,
  showItemInFolder: (filePath) => ipcRenderer.invoke('coppice:show-item-in-folder', filePath),
});
