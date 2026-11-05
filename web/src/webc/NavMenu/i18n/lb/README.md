# Navigatiounsmenü fir verschidden Écrangréissten ugepasst

- Desktop: Links ginn an horizontalen Fliesen ugewisen
- Mobile Versioun: Zesummebroch an de Menü Knäppchen, klickt fir de Sidebar Tirang eraus ze rutschen
- Route Aktivéierung: vergläicht automatesch den aktuelle Wee an den Hashwäert, a füügt den Numm vun der Aktivéierungsstatus un de passende Link un `A`
- Automatesch zoumaachen: Wann de mobilen Terminal op e Link oder den externen Hannergrond vun der Sidebar klickt, mécht d'Sidebar automatesch zou.

## benotzen

```html
<c-nav-menu>
  <a href="/">virun Säit</a>
  <a href="/doc">Dokument</a>
  <a href="/about">iwwer</a>
</c-nav-menu>
```

## illustréieren

Wann d'Komponente initialiséiert gëtt, wann et kee `c-side` am Kannerelement ass, gëtt de mobilen Menü Knäppchen an `c-side` Sidebar automatesch erstallt, an all `a` Tags ginn an d'Sidebar geréckelt.

## Stil Klass

- **A**: Den aktivéierte Link wäert den Numm vun der Klass `A` automatesch derbäisetzen