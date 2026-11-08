# Vaihda automaattisesti navigointivalikkoa ja paluupainiketta

- Kääri alatason elementit automaattisesti luodaksesi navigointivalikon ja palautuspainikkeen rakenteita
- Seuraa automaattisesti reitityshistoriaa ja laske edellinen paluupolku
- Näytä navigointivalikko, kun reitti vastaa valikon kohtaa, muussa tapauksessa näytä takaisin-painike

## Käytä demoa

```html
<c-nav-l>
  <a href="/">etusivulle</a>
  <a href="/doc">asiakirja</a>
</c-nav-l>
```

## tyyliluokka

- `B`: aktivoi paluupainikkeen tilan. Näytä Takaisin-painike ja piilota navigointivalikko. Vaihtuu automaattisesti komponentin sisällä reitityksen perusteella.