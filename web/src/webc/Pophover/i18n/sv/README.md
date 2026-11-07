# flytande bubbla prompt

Visa information om svävningsverktygstips när musen hålls eller elementet är aktiverat.

- Baserad på inbyggd webbkomponent, lätt och högpresterande
- Stöder adaptiv rumslig positionering i fyra riktningar (class="top/bottom/left/right")
- Den använder en svart frostat glastexturgradientbakgrund och vita tecken, med utsökta fysiska kantkonturer.
- Inbyggd positionsanpassad beräkning, justerar automatiskt popup-riktningen enligt det återstående utrymmet i visningsporten
- Använd `slot="pop"` för att visa innehåll för flytande meddelanden

## Använd demon

```html
<!-- Beräkna riktning automatiskt (rekommenderas, inget behov av att deklarera klass) -->
<c-pophover>
  <button>automatisk</button>
  <div slot="pop">Det här är ett bubbeltips</div>
</c-pophover>

<!-- fast riktning -->
<c-pophover class="left">
  <button>Vänster</button>
  <div slot="pop">Det här är ett bubbeltips</div>
</c-pophover>

<c-pophover class="right">
  <button>rätt</button>
  <div slot="pop">Det här är ett bubbeltips</div>
</c-pophover>

<c-pophover class="bottom">
  <button>Ner</button>
  <div slot="pop">Det här är ett bubbeltips</div>
</c-pophover>

<c-pophover class="top">
  <button>överlägsen</button>
  <div slot="pop">Det här är ett bubbeltips</div>
</c-pophover>
```

## CSS-variabler

Anpassade stilar stöds via följande CSS-variabler:

| variabelnamn | standardvärde | illustrera |
| :--- | :--- | :--- |
| `--pophover-gap` | `8px` | Utrymmet mellan bubblans verktygstips och det utlösande elementet |
| `--pophover-margin` | `24px` | Viewport edge säkerhetsmarginal under adaptiv positionsberäkning |
| `--pophover-bg-top` | `#18181ce0` | Bubbla gradient bakgrund toppfärg |
| `--pophover-bg-mid` | `#121215e5` | Bakgrundsfärgen på pilarna på bubblans vänstra och högra sida |
| `--pophover-bg-bottom` | `#0c0c0feb` | Färgen på de nedre och övre och nedre sidopilarna på bubbelgradientbakgrunden |

## Ikonverktygstipsstil (`.Ico`)

För flytande promptknappar av ren ikontyp (som "Redigera", "Radera", etc.), kan du använda en enhetlig ikon och hovringsstil genom att lägga till `.Ico` klassnamnet till den yttre behållaren eller komponenten själv.

### Exempel på struktur

```html
<b class="Edit Ico">
  <c-pophover>
    <a></a>
    <div slot="pop">redigera</div>
  </c-pophover>
</b>
```

Grundläggande layout, storlek (18x18px) och svävfärgsändringar har förenats i `.Ico > c-pophover > a`. Utvecklare behöver bara ange motsvarande ikonfil separat, till exempel:
```css
.Edit > c-pophover > a {
  mask-image: url("./svg/edit.svg")
}
```