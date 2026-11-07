# Changanua na utoe maandishi ya Markdown kwa HTML kwa usalama.

- **Usafishaji wa Usalama**: Jilinde dhidi ya mashambulizi ya XSS kwa kutumia visafishaji asili vya `setHTML` API au `DOMParser`
- **Uchimbaji wa Maudhui**: Soma kiotomatiki maudhui ya maandishi ya nafasi chaguomsingi wakati wa uanzishaji na uifanye
- **Mipangilio ya aina iliyojumuishwa**: orodha iliyowekwa awali, jedwali, uzuiaji wa msimbo, mitindo ya kisanduku cha kunukuu na ya onyo

## Tumia onyesho

### utoaji tuli

```html
<c-md>
# kichwa
Hiki ni kipande cha maudhui ya Markdown
</c-md>
```

### mgawo wa nguvu

```html
<c-md id="md"></c-md>

<script type="module">
import "webc.site/Md.js";
document.querySelector("#md").value = "#Kichwa kipya\nMaudhui mapya";
</script>
```

## Vigezo vya interface

Pitisha maandishi ya Markdown moja kwa moja ndani ya lebo (kama nafasi chaguomsingi), au usasishe kwa nguvu kupitia `value`.

## Maelezo ya mtindo

Darasa la mtindo wa `.Md` huongezwa kiotomatiki kipengele kinapoanzishwa. Inasaidia mpangilio ufuatao wa kisanduku cha onyo cha Markdown (kilichochochewa na syntax ya marejeleo):

- `[!NOTE]`:"Taarifa ya haraka
- `[!TIP]`: Mapendekezo/Vidokezo
- `[!IMPORTANT]`: Uangalifu muhimu
- `[!WARNING]`: Onyo la hatari
- `[!CAUTION]`: Onyo la hatari kubwa sana

## utaratibu wa usalama

Wakati wa kutoa, pigia API asili ya `setHTML` API kwanza ili kuingiza HTML kwa usalama. Ikiwa kivinjari hakiauni, kitashusha gredi kiotomatiki hadi kisafishaji kinachojirudia kulingana na `DOMParser` na kutekeleza sera ifuatayo ya usalama:
- Ondoa lebo zisizo salama (k.m. `script`, `iframe`, n.k.)
- Futa `on*` sifa za tukio
- Jilinde dhidi ya mashambulizi ya XSS kwa kuchuja sifa zinazoanza na itifaki hatari kama vile `javascript:`, `vbscript:`, na `data:` (isipokuwa `data:image/`).