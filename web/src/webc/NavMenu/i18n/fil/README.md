# Navigation menu na inangkop sa iba't ibang laki ng screen

- Desktop: Ang mga link ay ipinapakita sa mga pahalang na tile
- Mobile na bersyon: I-collapse sa menu button, i-click upang i-slide palabas ang sidebar drawer
- Pag-activate ng ruta: awtomatikong ihambing ang kasalukuyang path at halaga ng hash, at magdagdag ng pangalan ng klase ng status ng activation sa tumutugmang link `A`
- Awtomatikong isara: Kapag nag-click ang mobile terminal sa isang link o sa panlabas na background ng sidebar, awtomatikong magsasara ang sidebar.

## gamitin

```html
<c-nav-menu>
  <a href="/">front page</a>
  <a href="/doc">dokumento</a>
  <a href="/about">tungkol sa</a>
</c-nav-menu>
```

## ilarawan

Kapag nasimulan ang component, kung walang `c-side` sa child element, awtomatikong malilikha ang button ng menu ng mobile at `c-side` sidebar, at lahat ng `a` tag ay ililipat sa sidebar.

## klase ng istilo

- **A**: Awtomatikong idaragdag ng naka-activate na link ang pangalan ng klase `A`