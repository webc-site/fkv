# Parsige ja renderdage Markdowni tekst turvaliselt HTML-i.

- **Turvapuhastus**: kaitske XSS-i rünnakute eest, kasutades oma `setHTML` API või `DOMParser` puhastusvahendeid
- **Sisu ​​ekstraheerimine**: vaikepesa tekstisisu lugemine automaatselt initsialiseerimise ajal ja selle renderdamine
- **Sisseehitatud ladu**: eelseadistatud loendi, tabeli, koodiploki, tsitaadi ja hoiatuskasti stiilid

## Kasutage demo

### staatiline renderdamine

```html
<c-md>
# pealkiri
See on osa Markdowni sisust
</c-md>
```

### dünaamiline ülesanne

```html
<c-md id="md"></c-md>

<script type="module">
import "webc.site/Md.js";
document.querySelector("#md").value = "#Uus pealkiri\nUus sisu";
</script>
```

## Liidese parameetrid

Edastage Markdowni tekst otse märgendis (vaikepesa) või värskendage dünaamiliselt `value` kaudu.

## Stiili kirjeldus

Stiiliklass `.Md` lisatakse komponendi lähtestamisel automaatselt. Toetab järgmist Markdowni laiendatud hoiatuskasti paigutust (käivitab viitesüntaksi):

- `[!NOTE]`: Kiire teave
- `[!TIP]`: soovitused/näpunäited
- `[!IMPORTANT]`: oluline tähelepanu
- `[!WARNING]`: riskihoiatus
- `[!CAUTION]`: väga suure riski hoiatus

## turvamehhanism

Renderdamisel helistage esmalt natiivsele `setHTML` API-le, et HTML-i ohutult sisestada. Kui brauser seda ei toeta, läheb see automaatselt alla rekursiivsele puhastile, mis põhineb `DOMParser` ja rakendab järgmise turvapoliitika:
- Eemaldage mitteturvalised sildid (nt `script`, `iframe` jne)
- Kustutage `on*` sündmuse atribuudid
- Kaitske XSS-i rünnakute eest, filtreerides atribuute, mis algavad ohtlikest protokollidest, nagu `javascript:`, `vbscript:` ja `data:` (v.a `data:image/`).