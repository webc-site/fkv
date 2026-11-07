# lumulutang na bubble prompt

Ipakita ang impormasyon ng hover tooltip kapag naka-hover ang mouse o na-activate ang elemento.

- Batay sa katutubong Web Component, magaan at mataas ang pagganap
- Sinusuportahan ang adaptive spatial positioning sa apat na direksyon (class="top/bottom/left/right")
- Gumagamit ito ng black frosted glass texture gradient na background at mga puting character, na may katangi-tanging mga highlight ng tabas sa gilid ng katawan.
- Built-in na position adaptive na pagkalkula, awtomatikong inaayos ang pop-up na direksyon ayon sa natitirang espasyo sa viewport
- Gamitin ang `slot="pop"` upang ipakita ang lumulutang na prompt na nilalaman

## Gamitin ang demo

```html
<!-- Awtomatikong kalkulahin ang direksyon (inirerekomenda, hindi na kailangang magdeklara ng klase) -->
<c-pophover>
  <button>awtomatiko</button>
  <div slot="pop">Ito ay isang bubble tip</div>
</c-pophover>

<!-- nakapirming direksyon -->
<c-pophover class="left">
  <button>Kaliwa</button>
  <div slot="pop">Ito ay isang bubble tip</div>
</c-pophover>

<c-pophover class="right">
  <button>tama</button>
  <div slot="pop">Ito ay isang bubble tip</div>
</c-pophover>

<c-pophover class="bottom">
  <button>Pababa</button>
  <div slot="pop">Ito ay isang bubble tip</div>
</c-pophover>

<c-pophover class="top">
  <button>nakatataas</button>
  <div slot="pop">Ito ay isang bubble tip</div>
</c-pophover>
```

## Mga variable ng CSS

Ang mga custom na istilo ay sinusuportahan sa pamamagitan ng mga sumusunod na variable ng CSS:

| variable na pangalan | default na halaga | ilarawan |
| :--- | :--- | :--- |
| `--pophover-gap` | `8px` | Ang espasyo sa pagitan ng bubble tooltip at ng triggering element |
| `--pophover-margin` | `24px` | Ang margin ng kaligtasan sa gilid ng viewport sa panahon ng pagkalkula ng adaptive positioning |
| `--pophover-bg-top` | `#18181ce0` | Bubble gradient na kulay sa tuktok ng background |
| `--pophover-bg-mid` | `#121215e5` | Ang kulay ng background ng mga arrow sa kaliwa at kanang bahagi ng bubble |
| `--pophover-bg-bottom` | `#0c0c0feb` | Ang kulay ng mga arrow sa ibaba at itaas at ibabang bahagi ng background ng bubble gradient |

## Estilo ng tooltip ng icon (`.Ico`)

Para sa purong icon na uri ng lumulutang na mga pindutan ng prompt (tulad ng "I-edit", "Tanggalin", atbp.), maaari kang maglapat ng pinag-isang icon at istilo ng pag-hover sa pamamagitan ng pagdaragdag ng `.Ico` pangalan ng klase sa panlabas na lalagyan o bahagi mismo.

### Halimbawa ng istruktura

```html
<b class="Edit Ico">
  <c-pophover>
    <a></a>
    <div slot="pop">i-edit</div>
  </c-pophover>
</b>
```

Ang pangunahing layout, laki (18x18px), at mga pagbabago sa kulay ng hover ay pinag-isa sa `.Ico > c-pophover > a`. Kailangan lang ng mga developer na tukuyin ang kaukulang icon na file nang hiwalay, gaya ng:
```css
.Edit > c-pophover > a {
  mask-image: url("./svg/edit.svg")
}
```