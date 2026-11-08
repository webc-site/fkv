# Dokumentasi lan kode sumber kanggo komponen rendering utawa fungsi alat

- Tata letak responsif rong kolom, kanthi dokumen ditampilake ing sisih kiwa lan kode sumber ditampilake ing sisih tengen
- Dokumentasi lan kode sumber ndhukung tombol Sunting lan kode trabasan kanggo mlumpat menyang alamat sumber.
- Judhul lan introduksi ditampilake ing sisih ndhuwur, ndhukung terjemahan otomatis i18n.

## Gunakake demo

```html
<c-webc-js src="/src/js/dom.js" href="/doc/{code}/js/dom.md"></c-webc-js>
```

## Paramèter properti

- `src`: Alamat URL kode sumber.
- `href`: alamat URL markdown dokumen, ndhukung nggunakake placeholder `{code}` kanggo adaptasi karo basa saiki.