# Awtomatikong lumipat sa navigation menu at return button

- Awtomatikong i-wrap ang mga elemento ng bata upang bumuo ng menu ng nabigasyon at mga istruktura ng button na ibalik
- Awtomatikong subaybayan ang kasaysayan ng pagruruta at kalkulahin ang nakaraang landas ng pagbabalik
- Ipakita ang menu ng nabigasyon kapag tumugma ang ruta sa item ng menu, kung hindi man ay ipakita ang back button

## Gamitin ang demo

```html
<c-nav-l>
  <a href="/">front page</a>
  <a href="/doc">dokumento</a>
</c-nav-l>
```

## klase ng istilo

- `B`: ina-activate ang estado ng return button. Ipakita ang back button at itago ang navigation menu. Awtomatikong inilipat sa loob ng bahagi batay sa pagruruta.