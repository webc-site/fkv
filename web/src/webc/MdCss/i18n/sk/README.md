# Typografické štýly optimalizované pre zážitok z čítania

- **Základná sadzba**: Prispôsobte sa nadpisom, odsekom, tučnému písmu, kurzíve, prečiarknutiu, vloženému kódu, obrázkom a deliacim čiaram.
- ** Animácia odkazu**: Zobrazte animáciu podčiarknutia pri umiestnení kurzora myši.
- **Zoznam projektov**: Neusporiadané zoznamy používajú odrážky, usporiadané zoznamy používajú zaokrúhlené sériové čísla na pozadí a podporujú viacúrovňové vnorenie.
- **Zoznam úloh**: Poskytuje štýl začiarkavacieho políčka, podporuje stav úloh a dokončenia.
- **Tipy pre varovanie**: Podporuje päť typov polí s upozornením: Poznámka, Tip, Dôležité, Varovanie a Pozor.
- **Tabuľky a kód**: Poskytuje zvýraznenie riadkov tabuľky umiestnením kurzora myši a bloky kódu s tieňmi a gradientným pozadím.

## Použite demo

```html
<div class="Md">
  <h1>titul</h1>
  <p>Text odseku a <a href="#">odkaz</a></p>
  <hr>
  <h2>Titul druhej úrovne</h2>
  <ul><li><input type="checkbox" checked="" disabled=""> Položka úlohy</li></ul>
  <blockquote class="q note">
    <b class="qt"><i class="qi"></i>`<p>Obsah poznámky</p>
  </blockquote>
</div>
```

## štýlová trieda

Štýly sa aplikujú na prvky HTML v kontajneri `.Md`.

### Štýl výstražného poľa

Pridajte nasledujúci názov triedy do prvku `blockquote` s názvom triedy `.q`, aby ste prepli pole upozornenia:

- `note`: Poznámky
- `tip`: Tip
- `important`: Dôležité
- `warning`: varovanie
- `caution`: Poznámka