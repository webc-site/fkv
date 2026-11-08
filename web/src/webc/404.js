import i18nUrl, { MSG, HOME } from "./404/I18N.js";
import "./404/_.styl";
import { B, D, cE, newEl, newLi, clearDom } from "../js/dom.js";
import { On } from "../js/On.js";
import { fetchLang } from "../js/fetchLang.js";

const fLang = fetchLang(i18nUrl),
  LOOK_FOCUS = 0,
  LOOK_ROLL = 1,
  ANGRY = "angry",
  FURIOUS = "furious",
  EYE = "eye",
  PUPIL = "pupil",
  B_TAG = "b",
  BURNING_CURSOR = "burning-cursor",
  NO_CURSOR = "no-cursor",
  PI2 = Math.PI * 2,
  COLOR_RED = "#ff453a",
  TYPE_SMOKE = 0,
  TYPE_EXPLOSION = 1,
  TYPE_FIRE = 2,
  TYPE_SHOCKWAVE = 3,
  MAX_DROP_Y = 100,
  ANGRY_DURATION = 2500,
  LASER_DURATION = 1500,
  SLEEP_DELAY = 1500,
  EYE_RESTORE_DELAY = 2200,
  RESPAWN_DELAY = 3500,
  PARTICLE_POOL_LI = [],
  EYE_KEY_LI = ["bx", "by", "px", "py", "gx", "gy"],
  polar = (angle, dist) => [Math.cos(angle) * dist, Math.sin(angle) * dist],
  spawn = (type, x, y, vx, vy, life, size, color, gravity = 0, drag = 1, max_life = life) =>
    Object.assign(PARTICLE_POOL_LI.pop() || {}, {
      type,
      x,
      y,
      vx,
      vy,
      life,
      max_life,
      size,
      color,
      gravity,
      drag,
      radius: 5,
      speed: 5
    }),
  recycle = (p) => {
    PARTICLE_POOL_LI.push(p);
  },
  drop = (state, step) => {
    state.vy += 0.4 * step;
    state.by += state.vy * step;
    state.bx += state.vx * step;
    if (state.by > MAX_DROP_Y) {
      state.by = MAX_DROP_Y;
      state.vy = -state.vy * 0.7;
      state.bx += state.vx * 0.5 * step;
      state.vx *= 0.85;
    }
  },
  styles = (el, list_li, cache) => {
    const { style } = el;
    list_li.forEach(([name, val]) => {
      const rounded = Math.round(val * 100) / 100;
      if (cache[name] !== rounded) {
        cache[name] = rounded;
        style.setProperty(name, rounded);
      }
    });
  },
  write = (el, { bx, by, px = 0, py = 0, gx = 0, gy = 0, cache }, s = 1) => {
    styles(
      el,
      [
        ["--bx", bx],
        ["--by", by],
        ["--px", px],
        ["--py", py],
        ["--s", s],
        ["--gx", gx],
        ["--gy", gy]
      ],
      cache
    );
  },
  track = (arr, m_x, m_y, noise_x, noise_y) => {
    const dist = Math.hypot(m_x, m_y),
      f_8 = 8 / (dist + 8),
      f_5 = 5 / (dist + 5),
      f_4 = 4 / (dist + 4);
    arr.splice(
      0,
      7,
      m_x * f_8 + noise_x,
      m_y * f_8 + noise_y,
      m_x * f_5 + noise_x * 0.6,
      m_y * f_5 + noise_y * 0.6,
      m_x * f_4,
      m_y * f_4,
      dist
    );
  },
  look = (arr, lx, ly) => {
    arr.splice(0, 7, lx, ly, lx * 0.625, ly * 0.625, lx * 0.5, ly * 0.5, 0);
  },
  eye = (state, target_arr, s, lerp_f) => {
    let dist_sq = 0;
    for (let i = 0; i < 6; ++i) {
      const key = EYE_KEY_LI[i],
        diff = (target_arr[i] - state[key]) * lerp_f;
      state[key] += diff;
      dist_sq += diff * diff;
    }
    write(state.el, state, s);
    return dist_sq < 1e-6;
  },
  draw = (ctx, x, y, angle) => {
    ctx.save();
    ctx.translate(x, y);
    ctx.rotate(angle);
    ctx.shadowColor = COLOR_RED;
    ctx.shadowBlur = 2;
    ctx.beginPath();
    ctx.moveTo(0, 0);
    [
      [0, 17],
      [4.5, 13],
      [8.5, 21],
      [11, 20],
      [7, 12],
      [12, 12]
    ].forEach(([px, py]) => ctx.lineTo(px, py));
    ctx.closePath();
    ctx.fillStyle = "#1a1a1a";
    ctx.fill();
    ctx.strokeStyle = COLOR_RED;
    ctx.lineWidth = 1;
    ctx.stroke();
    ctx.restore();
  },
  explode = (particle_li, x, y, count = 60) => {
    particle_li.push(spawn(TYPE_SHOCKWAVE, x, y, 0, 0, 300, 0, "", 0, 1, 300));
    const smoke_limit = Math.round(count / 4);
    for (let i = 0; i < count; ++i) {
      const is_smoke = i < smoke_limit,
        angle = Math.random() * PI2,
        speed = is_smoke ? 0.5 + Math.random() * 1.5 : 3 + Math.random() * 10,
        [vx, vy] = polar(angle, speed);

      particle_li.push(
        is_smoke
          ? spawn(
              TYPE_SMOKE,
              x,
              y,
              vx,
              vy - 1,
              800 + Math.random() * 600,
              5 + Math.random() * 5,
              "rgba(80, 80, 80, 0.35)",
              -0.04,
              0.94,
              1400
            )
          : spawn(
              TYPE_EXPLOSION,
              x,
              y,
              vx,
              vy - 2,
              500 + Math.random() * 500,
              2 + Math.random() * 4,
              "hsl(" + (10 + Math.random() * 35) + ", 100%, " + (50 + Math.random() * 30) + "%)",
              0.18,
              0.96,
              1000
            )
      );
    }
  },
  build = (eye_el) => {
    const create = (tag) => Object.assign(newEl(tag), { className: tag }),
      [w, b, p, g, sp] = ["wrap", "ball", PUPIL, "glare", "sparkle"].map(create);
    p.append(g, sp);
    b.append(p);
    w.append(b);
    eye_el.append(w);
  },
  line = (el, x1, y1, x2, y2) => {
    el.setAttribute("x1", x1);
    el.setAttribute("y1", y1);
    el.setAttribute("x2", x2);
    el.setAttribute("y2", y2);
  },
  laser = (l_out, l_in, x, y, mx, my) => {
    [l_out, l_in].forEach((el) => line(el, x, y, mx, my));
  },
  escape = (res_li, dx, dy, step, tx, ty, cx, cy) => {
    const d = Math.hypot(dx, dy) || 1;
    let nx = tx - (dx / d) * 3.5 * step,
      ny = ty - (dy / d) * 3.5 * step;
    if (d < 220) {
      const factor = ((220 - d) / (220 * d)) * 10 * step;
      nx -= dx * factor;
      ny -= dy * factor;
    }
    const mx = 80,
      my = 100,
      w = window.innerWidth,
      h = window.innerHeight,
      ax = cx + nx,
      ay = cy + ny;
    if (ax < mx) {
      nx += (mx - ax) * 0.25 * step;
    } else if (ax > w - mx) {
      nx -= (ax - (w - mx)) * 0.25 * step;
    }
    if (ay < my) {
      ny += (my - ay) * 0.25 * step;
    } else if (ay > h - my) {
      ny -= (ay - (h - my)) * 0.25 * step;
    }
    res_li[0] = Math.max(60, Math.min(w - 60, cx + nx)) - cx;
    res_li[1] = Math.max(85, Math.min(h - 85, cy + ny)) - cy;
  },
  roll = (state, now) => {
    if (now > state.next_time && now > state.end_time) {
      state.mode = Math.random() > 0.5 ? LOOK_ROLL : LOOK_FOCUS;
      state.s = 1 + Math.random() * 0.16;
      state.end_time = now + 600 + Math.random() * 800;
      state.next_time = state.end_time + 1500 + Math.random() * 2500;

      if (state.mode === LOOK_ROLL) {
        state.start_time = now;
        state.r = 2.5 + Math.random() * 3.5;
        state.dir = Math.random() > 0.5 ? 1 : -1;
        state.start_angle = Math.random() * PI2;
        state.speed = (PI2 * (1 + Math.random() * 0.5)) / (state.end_time - now);
      } else {
        const angle = Math.random() * PI2,
          dist = 4 + Math.random() * 6.5;
        [state.x, state.y] = polar(angle, dist);
      }
    } else if (now > state.end_time) {
      state.x = state.y = 0;
      state.s = 1;
    } else if (state.mode === LOOK_ROLL) {
      const elapsed = now - state.start_time,
        curr_angle = state.start_angle + state.dir * elapsed * state.speed;
      [state.x, state.y] = polar(curr_angle, state.r);
    }
  },
  flow = (particle_li, dt, step, ctx) => {
    for (let i = particle_li.length - 1; i >= 0; --i) {
      const p = particle_li[i];
      p.life -= dt;
      if (p.life <= 0) {
        recycle(p);
        particle_li[i] = particle_li[particle_li.length - 1];
        particle_li.pop();
        continue;
      }
      const ratio = p.life / p.max_life;
      if (p.type === TYPE_SHOCKWAVE) {
        p.radius += p.speed * step;
        ctx.globalAlpha = ratio;
        ctx.beginPath();
        ctx.arc(p.x, p.y, p.radius, 0, PI2);
        ctx.strokeStyle = "rgba(255, 69, 58, " + 0.8 * ratio + ")";
        ctx.lineWidth = 2 * ratio + 0.5;
        ctx.stroke();
      } else {
        if (p.drag !== 1) {
          const f = 1 - (1 - p.drag) * step;
          p.vx *= f;
          p.vy *= f;
        }
        if (p.gravity) {
          p.vy += p.gravity * step;
        }
        p.x += p.vx * step;
        p.y += p.vy * step;
        ctx.globalAlpha = ratio;
        ctx.beginPath();
        ctx.arc(p.x, p.y, p.size * (p.type === TYPE_SMOKE ? 2 - ratio : ratio), 0, PI2);
        ctx.fillStyle = p.color;
        ctx.fill();
      }
    }
  },
  burn = (burnt, dt, step, canvas_h, particle_li, ctx) => {
    burnt.vy += 0.35 * step;
    burnt.x += burnt.vx * step;
    burnt.y += burnt.vy * step;
    burnt.angle += burnt.va * step;
    burnt.life -= dt;

    if (burnt.y > canvas_h + 50 || burnt.life <= 0) {
      return null;
    }
    if (Math.random() < 0.3 * step) {
      particle_li.push(
        spawn(
          TYPE_FIRE,
          burnt.x,
          burnt.y,
          (Math.random() - 0.5) * 1,
          -0.5 - Math.random() * 1,
          400 + Math.random() * 400,
          2 + Math.random() * 2,
          "rgba(100, 100, 100, 0.4)"
        )
      );
    }
    draw(ctx, burnt.x, burnt.y, burnt.angle);
    return burnt;
  },
  reset = (state) => {
    state.px = state.py = state.vx = state.vy = 0;
  };

let laser_svg = null,
  canvas_el = null,
  laser_l_out = null,
  laser_r_out = null,
  laser_l_in = null,
  laser_r_in = null,
  laser_burn = null;

const init = () => {
  if (!laser_svg) {
    laser_svg = D.createElementNS("http://www.w3.org/2000/svg", "svg");
    laser_svg.setAttribute("class", "lasers");
    laser_svg.style.cssText =
      "position:fixed;top:0;left:0;width:100vw;height:100vh;pointer-events:none;z-index:10001;display:none";
    laser_svg.innerHTML =
      '<defs><filter id="laser-glow" x="-50%" y="-50%" width="200%" height="200%"><feGaussianBlur stdDeviation="2.5" result="blur"/><feMerge><feMergeNode in="blur"/><feMergeNode in="SourceGraphic"/></feMerge></filter><radialGradient id="burn-glow"><stop offset="0%" stop-color="var(--color-white, #fff)"/><stop offset="35%" stop-color="var(--color-red, #ff453a)"/><stop offset="100%" stop-color="var(--color-red, #ff453a)" stop-opacity="0"/></radialGradient></defs><line id="laser-l-outer" stroke="var(--color-red, #ff453a)" stroke-width="4.5" filter="url(#laser-glow)" style="opacity:0"/><line id="laser-r-outer" stroke="var(--color-red, #ff453a)" stroke-width="4.5" filter="url(#laser-glow)" style="opacity:0"/><line id="laser-l-inner" stroke="var(--color-white, #fff)" stroke-width="1.5" style="opacity:0"/><line id="laser-r-inner" stroke="var(--color-white, #fff)" stroke-width="1.5" style="opacity:0"/><circle id="laser-burn" fill="url(#burn-glow)" style="opacity:0"/>';

    [laser_l_out, laser_r_out, laser_l_in, laser_r_in, laser_burn] = [
      "#laser-l-outer",
      "#laser-r-outer",
      "#laser-l-inner",
      "#laser-r-inner",
      "#laser-burn"
    ].map((s) => laser_svg.querySelector(s));
  }
  if (!canvas_el) {
    canvas_el = newEl("canvas");
    canvas_el.className = "effects-canvas";
    canvas_el.style.cssText =
      "position:fixed;top:0;left:0;width:100vw;height:100vh;pointer-events:none;z-index:10001";
  }
};

cE(
  "404",
  class extends HTMLElement {
    connectedCallback() {
      if (this.firstChild) return;

      let raf_id = 0,
        mouse_x = 0,
        mouse_y = 0,
        last_move = 0,
        s = 1,
        gr_x = 0,
        gr_y = 0,
        escape_x = 0,
        escape_y = 0,
        target_escape_x = 0,
        target_escape_y = 0,
        angry_start_time = 0,
        has_exploded = false,
        shake_intensity = 0,
        last_time = 0,
        look_timeout = null,
        is_sleeping = false,
        base_cx = 0,
        base_cy = 0,
        click_count = 0,
        is_dead = false,
        respawn_timeout = null,
        restore_eye_timeout = null,
        burnt_cursor = null,
        eye_drop = false,
        pupil_l = null,
        pupil_r = null;

      const look_state = {
          next_time: 0,
          end_time: 0,
          x: 0,
          y: 0,
          s: 1,
          mode: 0,
          start_time: 0,
          r: 0,
          dir: 1,
          speed: 0,
          start_angle: 0
        },
        toggleCursor = (add) =>
          [B, this].forEach((el) => {
            el.classList.remove(BURNING_CURSOR, NO_CURSOR);
            if (add) el.classList.add(add);
          }),
        updateMouse = (x, y) => {
          mouse_x = x;
          mouse_y = y;
          last_move = performance.now();
          wake();
        },
        _dummy = init(),
        ctx = canvas_el.getContext("2d"),
        [h1, btn, ghost_area, ghost, eye_box, eye_l, eye_r, shadow] = newLi(
          "h1",
          "a",
          "div",
          B_TAG,
          B_TAG,
          B_TAG,
          B_TAG,
          B_TAG
        ),
        initEye = (el) => ({
          bx: 0,
          by: 0,
          px: 0,
          py: 0,
          gx: 0,
          gy: 0,
          vx: 0,
          vy: 0,
          el,
          cache: {}
        }),
        eye_state_li = [initEye(eye_l), initEye(eye_r)],
        resizeCanvas = () => {
          canvas_el.width = window.innerWidth;
          canvas_el.height = window.innerHeight;
          laser_svg.setAttribute("width", window.innerWidth);
          laser_svg.setAttribute("height", window.innerHeight);
          const { left, width, top, height } = eye_box.getBoundingClientRect();
          base_cx = left + width / 2 - escape_x;
          base_cy = top + height / 2 - escape_y;
          if (last_move === 0) {
            mouse_x = base_cx;
            mouse_y = base_cy;
          }
        },
        particle_li = [],
        target_l_li = [0, 0, 0, 0, 0, 0, 0],
        target_r_li = [0, 0, 0, 0, 0, 0, 0],
        escape_res_li = [0, 0],
        eye_cache = {},
        ghost_cache = {},
        ghost_area_cache = {},
        unsub_li = [],
        wake = () => {
          if (look_timeout) {
            clearTimeout(look_timeout);
            look_timeout = null;
          }
          if (is_sleeping) {
            is_sleeping = false;
            last_time = performance.now();
            raf_id = requestAnimationFrame(loop);
          }
        },
        loop = (now) => {
          if (!last_time) last_time = now;
          const dt = Math.min(33.3, now - last_time),
            step = dt / 16.7,
            lerp_f = Math.min(1, 0.1 * step),
            curr_cx = base_cx + escape_x,
            curr_cy = base_cy + escape_y,
            dx = mouse_x - curr_cx,
            dy = mouse_y - curr_cy;
          last_time = now;

          let target_s = 1,
            target_rx = 0,
            target_ry = 0,
            is_angry = false,
            is_laser_active = false,
            sx = 0,
            sy = 0;

          if (angry_start_time > 0) {
            const elapsed = now - angry_start_time;
            if (elapsed < ANGRY_DURATION) {
              is_angry = true;
              if (elapsed < LASER_DURATION) {
                is_laser_active = true;
                shake_intensity = 1.0 + (elapsed / LASER_DURATION) * 3.0;

                escape(
                  escape_res_li,
                  dx,
                  dy,
                  step,
                  target_escape_x,
                  target_escape_y,
                  base_cx,
                  base_cy
                );
                target_escape_x = escape_res_li[0];
                target_escape_y = escape_res_li[1];
              } else {
                if (ghost.classList.contains(FURIOUS)) {
                  ghost.classList.remove(FURIOUS);
                }
                if (!has_exploded) {
                  has_exploded = true;
                  explode(particle_li, mouse_x, mouse_y, 60);
                  shake_intensity = 15.0;
                  toggleCursor();
                  toggleCursor(NO_CURSOR);

                  const dist = Math.hypot(dx, dy) || 1,
                    speed = 18 + Math.random() * 6;

                  burnt_cursor = {
                    x: mouse_x,
                    y: mouse_y,
                    vx: (dx / dist) * speed,
                    vy: (dy / dist) * speed,
                    va: (Math.random() - 0.5) * 0.6,
                    angle: Math.atan2(dy, dx) - Math.PI / 2,
                    life: 2000
                  };
                }
              }
            } else {
              angry_start_time = target_escape_x = target_escape_y = 0;
              ghost.classList.remove(ANGRY, FURIOUS);
              toggleCursor();
              eye_drop = false;
              click_count = 0;
            }
          }

          if (shake_intensity > 0.05) {
            sx = (Math.random() - 0.5) * shake_intensity;
            sy = (Math.random() - 0.5) * shake_intensity;
            this.style.transform = "translate(" + sx + "px, " + sy + "px)";
            shake_intensity *= Math.max(0, 1 - 0.12 * step);
          } else {
            this.style.transform = "";
          }

          if (now - last_move > SLEEP_DELAY && !is_angry) {
            roll(look_state, now);
            target_s = look_state.s;
            [target_l_li, target_r_li].forEach((arr) => look(arr, look_state.x, look_state.y));
          } else {
            const noise_x = Math.sin(now / 100) * 1.3 + Math.cos(now / 60) * 0.7,
              noise_y = Math.cos(now / 100) * 1.3 + Math.sin(now / 60) * 0.7;

            [
              [target_l_li, 17],
              [target_r_li, -17]
            ].forEach(([arr, offset]) => track(arr, dx + offset, dy, noise_x, noise_y));
            target_s = 1 + Math.max(0, 0.3 * (1 - ((target_l_li[6] + target_r_li[6]) * 0.5) / 400));
            target_rx = Math.max(-8, Math.min(8, -dy / 25));
            target_ry = Math.max(-10, Math.min(10, dx / 25));
          }

          s += (target_s - s) * lerp_f;
          gr_x += (target_rx - gr_x) * lerp_f;
          gr_y += (target_ry - gr_y) * lerp_f;
          escape_x += (target_escape_x - escape_x) * lerp_f;
          escape_y += (target_escape_y - escape_y) * lerp_f;

          let eyes_still = false;

          if (eye_drop) {
            eye_state_li.forEach((state) => {
              drop(state, step);
              write(state.el, state);
            });
          } else {
            eyes_still = eye_state_li
              .map((state, i) => eye(state, i ? target_r_li : target_l_li, s, lerp_f))
              .every(Boolean);
          }

          const [eye_state_l, eye_state_r] = eye_state_li;
          styles(
            eye_box,
            [
              ["--bx", (eye_state_l.bx + eye_state_r.bx) * 0.5],
              ["--by", (eye_state_l.by + eye_state_r.by) * 0.5]
            ],
            eye_cache
          );
          styles(
            ghost,
            [
              ["--rx", gr_x],
              ["--ry", gr_y]
            ],
            ghost_cache
          );
          styles(
            ghost_area,
            [
              ["--ex", escape_x],
              ["--ey", escape_y]
            ],
            ghost_area_cache
          );

          if (is_laser_active && pupil_l && pupil_r) {
            const rect_l = pupil_l.getBoundingClientRect(),
              rect_r = pupil_r.getBoundingClientRect();

            laser_svg.style.display = "";
            laser_svg.classList.add("active");
            [
              [laser_l_out, laser_l_in, rect_l],
              [laser_r_out, laser_r_in, rect_r]
            ].forEach(([l_out, l_in, { left, top, width, height }]) => {
              laser(l_out, l_in, left + width / 2, top + height / 2, mouse_x, mouse_y);
              l_out.style.opacity = 0.85;
              l_in.style.opacity = 1;
            });
            laser_burn.setAttribute("cx", mouse_x);
            laser_burn.setAttribute("cy", mouse_y);
            laser_burn.setAttribute("r", 12 + Math.sin(now / 30) * 3);
            laser_burn.style.opacity = 1;
          } else {
            laser_svg.classList.remove("active");
            laser_svg.style.display = "none";
            [laser_l_out, laser_r_out, laser_l_in, laser_r_in, laser_burn].forEach((el) => {
              if (el) el.style.opacity = 0;
            });
          }

          ctx.clearRect(0, 0, canvas_el.width, canvas_el.height);

          if (is_laser_active) {
            const fire_count = Math.round(3 * step);
            for (let i = 0; i < fire_count; ++i) {
              particle_li.push(
                spawn(
                  TYPE_FIRE,
                  mouse_x + (Math.random() - 0.5) * 8,
                  mouse_y + (Math.random() - 0.5) * 8,
                  (Math.random() - 0.5) * 2,
                  -1.5 - Math.random() * 2.5,
                  500 + Math.random() * 500,
                  3 + Math.random() * 4,
                  "hsl(" + (15 + Math.random() * 25) + ", 100%, " + (50 + Math.random() * 20) + "%)"
                )
              );
            }
          }

          flow(particle_li, dt, step, ctx);
          ctx.globalAlpha = 1;

          if (burnt_cursor) {
            burnt_cursor = burn(burnt_cursor, dt, step, canvas_el.height, particle_li, ctx);
            if (!burnt_cursor) toggleCursor();
          }

          const is_still =
            !eye_drop &&
            eyes_still &&
            Math.abs(target_s - s) < 0.001 &&
            Math.abs(target_rx - gr_x) < 0.001 &&
            Math.abs(target_ry - gr_y) < 0.001 &&
            Math.abs(target_escape_x - escape_x) < 0.001 &&
            Math.abs(target_escape_y - escape_y) < 0.001;

          if (!is_angry && particle_li.length === 0 && !burnt_cursor && is_still) {
            const is_under_delay = now - last_move < SLEEP_DELAY;
            if (is_under_delay || now > look_state.end_time) {
              is_sleeping = true;
              clearTimeout(look_timeout);
              look_timeout = setTimeout(
                wake,
                is_under_delay
                  ? SLEEP_DELAY - (now - last_move)
                  : Math.max(0, look_state.next_time - now)
              );
              return;
            }
          }

          raf_id = requestAnimationFrame(loop);
        };
      unsub_li.push(
        On(D, {
          mousemove: (e) => {
            if (mouse_x === e.clientX && mouse_y === e.clientY) return;
            updateMouse(e.clientX, e.clientY);
          }
        }),
        On(window, { resize: resizeCanvas }),
        fLang((i18n) => {
          h1.textContent = i18n[MSG];
          btn.textContent = i18n[HOME];
        }),
        On(ghost, {
          click: (e) => {
            if (is_dead) return;
            if (e.clientX !== undefined) {
              updateMouse(e.clientX, e.clientY);
            } else {
              wake();
            }
            ++click_count;

            if (click_count < 2) {
              eye_drop = false;
              ghost.classList.remove(ANGRY, FURIOUS);
              void ghost.offsetWidth;
              ghost.classList.add(ANGRY, FURIOUS);
              toggleCursor(BURNING_CURSOR);

              const angle = Math.random() * PI2,
                dist = 100 + Math.random() * 120;
              [target_escape_x, target_escape_y] = polar(angle, dist);

              angry_start_time = performance.now();
              has_exploded = false;
            } else {
              click_count = 0;
              is_dead = true;
              angry_start_time = 0;
              toggleCursor();
              const temp_ghost = newEl("div");
              explode(particle_li, base_cx + escape_x, base_cy + escape_y, 120);
              shake_intensity = 35.0;

              temp_ghost.className = "ghost " + ANGRY + " " + FURIOUS;
              temp_ghost.style.cssText =
                "position:absolute;top:0;left:0;width:100%;height:100%;pointer-events:none;background-image:none;animation:none;";
              temp_ghost.append(eye_box);
              ghost_area.append(temp_ghost);

              ghost.style.opacity = 0;
              shadow.style.opacity = 0;
              ghost.style.pointerEvents = "none";

              eye_drop = true;
              eye_state_li.forEach((state) => {
                const speed = 6 + Math.random() * 4,
                  angle = Math.PI * (0.15 + Math.random() * 0.7),
                  [vx, vy] = polar(angle, speed);
                state.vx = vx;
                state.vy = -vy;
              });

              target_escape_x = target_escape_y = 0;

              restore_eye_timeout = setTimeout(() => {
                temp_ghost.classList.remove(ANGRY, FURIOUS);
                ghost.classList.remove(ANGRY, FURIOUS);
              }, EYE_RESTORE_DELAY);

              respawn_timeout = setTimeout(() => {
                is_dead = false;
                eye_drop = false;
                click_count = 0;
                eye_state_li.forEach((s) => {
                  reset(s);
                  write(s.el, s);
                });
                temp_ghost.remove();
                ghost.append(eye_box);
                ghost.style.opacity = 1;
                shadow.style.opacity = 1;
                ghost.style.pointerEvents = "auto";
                shake_intensity = 5.0;
                wake();
              }, RESPAWN_DELAY);
            }
          }
        })
      );

      btn.className = "Btn Main";
      btn.href = "/";

      [ghost_area, ghost, eye_box, eye_l, eye_r, shadow].forEach(
        (el, i) => (el.className = ["ghost-area", "ghost", "eyes", EYE, EYE, "shadow"][i])
      );
      ghost.style.transition = shadow.style.transition = "opacity 0.4s ease";

      [eye_l, eye_r].forEach((el) => {
        build(el);
        eye_box.append(el);
      });
      [pupil_l, pupil_r] = [eye_l, eye_r].map((el) => el.querySelector("." + PUPIL));
      ghost.append(eye_box);
      ghost_area.append(ghost, shadow);

      this.append(btn, h1, ghost_area);
      B.append(laser_svg, canvas_el);

      resizeCanvas();
      const resize_timeout = setTimeout(resizeCanvas, 100);

      raf_id = requestAnimationFrame(loop);

      this.$ = () => {
        unsub_li.forEach((fn) => fn());
        toggleCursor();
        [laser_svg, canvas_el].forEach((el) => el.remove());
        [respawn_timeout, restore_eye_timeout, look_timeout, resize_timeout].forEach(clearTimeout);
        particle_li.forEach(recycle);
        particle_li.length = 0;
        cancelAnimationFrame(raf_id);
        clearDom(this);
      };
    }

    disconnectedCallback() {
      if (this.$) {
        this.$();
      }
    }
  }
);
