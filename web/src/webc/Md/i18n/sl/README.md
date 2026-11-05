# Varno razčlenite in upodabljajte Markdown besedilo v HTML.

- **Varnostna sanacija**: Zaščitite se pred napadi XSS z izvornim `setHTML` API-jem ali `DOMParser` čistilci
- **Izvleček vsebine**: samodejno preberi besedilno vsebino privzete reže med inicializacijo in jo upodobi
- **Vgrajeno stavljanje**: prednastavljeni seznam, tabela, kodni blok, slogi citatov in opozorilnih polj

## Uporabite demo

### statično upodabljanje

```html
<c-md>
# naslov
To je del vsebine Markdown
</c-md>
```

### dinamična dodelitev

```html
<c-md id="md"></c-md>

<script type="module">
import "webc.site/Md.js";
document.querySelector("#md").value = "#Nov naslov\nNova vsebina";
</script>
```

## Parametri vmesnika

Besedilo Markdown posredujte neposredno znotraj oznake (kot privzeta reža) ali pa ga dinamično posodobite prek `value`.

## Opis sloga

Razred sloga `.Md` se samodejno doda, ko je komponenta inicializirana. Podpira naslednjo postavitev razširjenega opozorilnega polja Markdown (sproži referenčna sintaksa):

- `[!NOTE]`：Takojšnje informacije
- `[!TIP]`: Predlogi/namigi
- `[!IMPORTANT]`: Pomembna pozornost
- `[!WARNING]`: Opozorilo o tveganju
- `[!CAUTION]`: Opozorilo o izjemno visokem tveganju

## varnostni mehanizem

Pri upodabljanju najprej pokličite izvorni `setHTML` API, da varno vstavite HTML. Če brskalnik tega ne podpira, se bo samodejno znižal na rekurzivni čistilec, ki temelji na `DOMParser`, in implementiral naslednjo varnostno politiko:
- Odstranite oznake, ki niso varne (npr. `script`, `iframe` itd.)
- Počisti `on*` atribute dogodka
- Zaščitite se pred napadi XSS s filtriranjem atributov, ki se začnejo z nevarnimi protokoli, kot so `javascript:`, `vbscript:` in `data:` (razen `data:image/`).