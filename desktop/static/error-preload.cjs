const { contextBridge, ipcRenderer } = require('electron');

contextBridge.exposeInMainWorld('coppiceShellError', {
  getDetails: () => ipcRenderer.invoke('coppice:error-details'),
  action: (name) => ipcRenderer.invoke('coppice:error-action', name),
});
