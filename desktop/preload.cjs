const { contextBridge, ipcRenderer } = require('electron');

contextBridge.exposeInMainWorld('coppiceDesktop', {
  pickDirectory: () => ipcRenderer.invoke('coppice:pick-directory'),
  appInfo: () => ipcRenderer.invoke('coppice:app-info'),
  getUpdateInfo: () => ipcRenderer.invoke('coppice:update-info'),
  onUpdateInfo: (listener) => {
    const wrapped = (_event, info) => listener(info);
    ipcRenderer.on('coppice:update-info', wrapped);
    return () => ipcRenderer.removeListener('coppice:update-info', wrapped);
  },
  getUpdateSettings: () => ipcRenderer.invoke('coppice:update-settings-get'),
  setUpdateSettings: (settings) => ipcRenderer.invoke('coppice:update-settings-set', settings),
  checkForUpdates: () => ipcRenderer.invoke('coppice:check-for-updates'),
  restartToUpdate: () => ipcRenderer.invoke('coppice:restart-to-update'),
  platform: process.platform,
  showItemInFolder: (filePath) => ipcRenderer.invoke('coppice:show-item-in-folder', filePath),
});
