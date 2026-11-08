# Oxu təcrübəsi üçün optimallaşdırılmış tipoqrafiya üslubları

- **Əsas yazı yazısı**: Başlıqlara, paraqraflara, qalın, kursiv, üstü çizilmiş, sətirli kod, şəkillər və ayırıcı sətirlərə uyğunlaşdırın.
- **Link animasiyası**: Üzəri tutarkən alt xətt animasiyasını göstərin.
- **Layihə siyahısı**: Sıralanmamış siyahılar güllələrdən istifadə edir, sıralanmış siyahılar dairəvi arxa fon seriya nömrələrindən istifadə edir və çoxsəviyyəli yerləşdirməni dəstəkləyir.
- **Tapşırıq Siyahısı**: Yoxlama qutusu tərzini təmin edir, görüləcək işləri və tamamlanmış statusu dəstəkləyir.
- **Xəbərdarlıq göstərişləri**: Beş növ xəbərdarlıq qutusunu dəstəkləyir: Qeyd, İpucu, Vacib, Xəbərdarlıq və Diqqət.
- **Cədvəllər və Kod**: Cədvəl cərgəsinin üstünə vurulmasını və kölgələr və gradient fonları olan kod bloklarını təmin edir.

## Demodan istifadə edin

```html
<div class="Md">
  <h1>başlıq</h1>
  <p>Abzas mətni və <a href="#">link</a></p>
  <hr>
  <h2>İkinci səviyyəli başlıq</h2>
  <ul><li><input type="checkbox" checked="" disabled=""> Tapşırıq elementi</li></ul>
  <blockquote class="q note">
    <b class="qt"><i class="qi"></i>`<p>Qeyd məzmunu</p>
  </blockquote>
</div>
```

## stil sinfi

Üslublar `.Md` konteyneri altında HTML elementlərinə tətbiq edilir.

### Xəbərdarlıq qutusu üslubu

Xəbərdarlıq qutusunu dəyişdirmək üçün `blockquote` elementinə aşağıdakı sinif adını `.q` sinif adı ilə əlavə edin:

- `note`: Qeydlər
- `tip`: İpucu
- `important`: Vacibdir
- `warning`: xəbərdarlıq
- `caution`: Qeyd