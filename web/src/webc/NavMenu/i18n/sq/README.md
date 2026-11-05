# Menuja e navigimit e përshtatur për madhësi të ndryshme ekrani

- Desktop: Lidhjet shfaqen në pllaka horizontale
- Versioni celular: Palos në butonin e menysë, kliko për të rrëshqitur jashtë sirtarin e shiritit anësor
- Aktivizimi i rrugës: krahasoni automatikisht shtegun aktual dhe vlerën hash dhe shtoni emrin e klasës së statusit të aktivizimit në lidhjen që përputhet `A`
- Mbyll automatikisht: Kur terminali celular klikon në një lidhje ose në sfondin e jashtëm të shiritit anësor, shiriti anësor mbyllet automatikisht.

## përdorni

```html
<c-nav-menu>
  <a href="/">faqe e parë</a>
  <a href="/doc">dokument</a>
  <a href="/about">rreth</a>
</c-nav-menu>
```

## ilustrojnë

Kur komponenti inicializohet, nëse nuk ka `c-side` në elementin fëmijë, butoni i menysë celulare dhe shiriti anësor `c-side` do të krijohen automatikisht dhe të gjitha etiketat `a` do të zhvendosen në shiritin anësor.

## klasë stili

- **A**: Lidhja e aktivizuar do të shtojë automatikisht emrin e klasës `A`