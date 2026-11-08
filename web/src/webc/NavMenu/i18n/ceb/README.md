# Ang menu sa nabigasyon gipahiangay sa lainlaing gidak-on sa screen

- Desktop: Ang mga link gipakita sa pinahigda nga mga tile
- Mobile nga bersyon: I-collapse sa menu button, i-klik aron i-slide pagawas ang sidebar drawer
- Pag-aktibo sa ruta: awtomatik nga itandi ang kasamtangan nga agianan ug hash nga kantidad, ug idugang ang ngalan sa klase sa status sa pagpaaktibo sa katugbang nga link `A`
- Awtomatikong pagsira: Kung ang mobile terminal mag-klik sa usa ka link o sa gawas nga background sa sidebar, ang sidebar awtomatikong magsira.

## paggamit

```html
<c-nav-menu>
  <a href="/">atubangan nga panid</a>
  <a href="/doc">dokumento</a>
  <a href="/about">mahitungod sa</a>
</c-nav-menu>
```

## iilustrar

Sa diha nga ang component gisugdan, kung walay `c-side` sa child element, ang mobile menu button ug `c-side` sidebar awtomatikong himoon, ug ang tanang `a` tag ibalhin ngadto sa sidebar.

## estilo nga klase

- **A**: Ang gi-aktibo nga link awtomatikong idugang ang ngalan sa klase `A`