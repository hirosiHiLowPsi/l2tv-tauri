"use strict";

(() => {
  const invoke = window.__TAURI__?.core?.invoke;
  if (typeof invoke !== "function") {
    return;
  }

  window.lr2irDesktop = Object.freeze({
    getApiToken: () => Promise.resolve("tauri-native"),
    requestApi: (path, body) => invoke("api_request", { path, body: body ?? {} }),
    pickFile: (options) => invoke("pick_file", { options: options ?? {} }),
    pickDirectory: (options) => invoke("pick_directory", { options: options ?? {} }),
    saveImage: (options) => invoke("save_image", { options: options ?? {} }),
    exportDataTransfer: (options) => invoke("export_data_transfer", { options: options ?? {} }),
    importDataTransfer: () => invoke("import_data_transfer"),
    fetchStellaverseRival: (options) => invoke("fetch_stellaverse_rival", { options: options ?? {} }),
    fetchStellaverseRankings: (options) => invoke("fetch_stellaverse_rankings", { options: options ?? {} })
  });
})();
