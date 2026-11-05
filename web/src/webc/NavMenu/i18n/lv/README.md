# Navigācijas izvēlne pielāgota dažādiem ekrāna izmēriem

- Darbvirsma: saites tiek parādītas horizontālā veidā
- Mobilā versija: sakļaut izvēlnes pogu, noklikšķiniet, lai izbīdītu sānjoslas atvilktni
- Maršruta aktivizēšana: automātiski salīdziniet pašreizējo ceļu un jaucējvērtību un pievienojiet aktivizēšanas statusa klases nosaukumu atbilstošajai saitei `A`
- Automātiski aizvērt: kad mobilais terminālis noklikšķina uz saites vai sānjoslas ārējā fona, sānjosla tiek automātiski aizvērta.

## izmantot

```html
<c-nav-menu>
  <a href="/">sākuma lapa</a>
  <a href="/doc">dokumentu</a>
  <a href="/about">par</a>
</c-nav-menu>
```

## ilustrēt

Ja komponents ir inicializēts, ja bērna elementā nav `c-side`, tiks automātiski izveidota mobilās izvēlnes poga un `c-side` sānjosla, un visi `a` tagi tiks pārvietoti uz sānjoslu.

## stila klase

- **A**: aktivizētā saite automātiski pievienos klases nosaukumu `A`