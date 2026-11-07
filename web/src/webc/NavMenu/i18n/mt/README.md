# Menu tan-navigazzjoni adattat għal daqsijiet differenti tal-iskrin

- Desktop: Il-links jintwerew f'madum orizzontali
- Verżjoni mobbli: Kollass fil-buttuna tal-menu, ikklikkja biex tiżżerżaq 'il barra mill-kexxun tal-bar tal-ġenb
- Attivazzjoni tar-rotta: qabbel awtomatikament il-mogħdija attwali u l-valur tal-hash, u żid l-isem tal-klassi tal-istatus tal-attivazzjoni mal-link li taqbel `A`
- Agħlaq awtomatikament: Meta t-terminal mobbli jikklikkja fuq link jew l-isfond estern tal-sidebar, il-sidebar tagħlaq awtomatikament.

## użu

```html
<c-nav-menu>
  <a href="/">paġna ta’ quddiem</a>
  <a href="/doc">dokument</a>
  <a href="/about">dwar</a>
</c-nav-menu>
```

## juru

Meta l-komponent jiġi inizjalizzat, jekk ma jkunx hemm `c-side` fl-element tat-tfal, il-buttuna tal-menu mobbli u l-`c-side` sidebar se jinħolqu awtomatikament, u t-tags kollha `a` se jiġu mċaqalqa fil-sidebar.

## klassi tal-istil

- **A**: Il-link attivat se żżid awtomatikament l-isem tal-klassi `A`