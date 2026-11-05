# Typografické styly optimalizované pro zážitek ze čtení

- **Základní sazba**: Přizpůsobte se nadpisům, odstavcům, tučnému písmu, kurzívě, přeškrtnutí, vloženému kódu, obrázkům a dělicím čarám.
- **Animace odkazu**: Zobrazí animaci podtržení při umístění kurzoru.
- **Seznam projektů**: Neuspořádané seznamy používají odrážky, uspořádané seznamy používají zaokrouhlená sériová čísla na pozadí a podporují víceúrovňové vnořování.
- **Seznam úkolů**: Poskytuje styl zaškrtávacích políček, podporuje stav úkolů a dokončení.
- **Tipy pro varování**: Podporuje pět typů varovných polí: Poznámka, Tip, Důležité, Varování a Pozor.
- **Tabulky a kód**: Poskytuje zvýraznění řádku při najetí myší a bloky kódu se stíny a gradientním pozadím.

## Použijte demo

```html
<div class="Md">
  <h1>titul</h1>
  <p>Text odstavce a <a href="#">odkaz</a></p>
  <hr>
  <h2>Titul druhé úrovně</h2>
  <ul><li><input type="checkbox" checked="" disabled=""> Položka úkolu</li></ul>
  <blockquote class="q note">
    <b class="qt"><i class="qi"></i>`<p>Obsah poznámky</p>
  </blockquote>
</div>
```

## stylová třída

Styly se aplikují na prvky HTML v kontejneru `.Md`.

### Styl pole upozornění

Přidáním následujícího názvu třídy do prvku `blockquote` s názvem třídy `.q` přepnete pole upozornění:

- `note`: Poznámky
- `tip`: Tip
- `important`: Důležité
- `warning`: upozornění
- `caution`: Poznámka