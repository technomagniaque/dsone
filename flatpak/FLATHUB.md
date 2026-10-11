# dsone — Flatpak et soumission à Flathub

Identifiant : `io.github.technomagniaque.dsone` (fork de SONE, `io.github.lullabyX.sone`).

## Avertissement important

> **Flathub peut refuser dsone, ou le retirer plus tard.** Le client Deezer
> utilise l'API privée de Deezer et **déchiffre les flux audio** côté client.
> Les règles de Flathub excluent les applications qui enfreignent les
> conditions d'utilisation d'un service ou contournent une protection ; un
> relecteur, Deezer ou un tiers peut le signaler à tout moment, avant ou après
> publication. Upstream (SONE) n'est accepté que pour TIDAL, via son API OAuth
> officielle : cela ne garantit rien pour dsone.
>
> Dites-le franchement dans la PR (voir plus bas) plutôt que de le découvrir
> lors d'un retrait.

### Plan B (indépendant de Flathub)

1. **Bundle `.flatpak` sur GitHub** : `.github/workflows/flatpak-bundle.yml`
   construit `dsone-x86_64.flatpak` (et `dsone-aarch64.flatpak`) à chaque tag
   `v*` et les attache à la Release. Installation :
   ```sh
   flatpak install --user flathub org.gnome.Platform//50   # runtime, une fois
   flatpak install --user ./dsone-x86_64.flatpak
   ```
   Inconvénient : pas de mise à jour automatique.
2. **Dépôt Flatpak auto-hébergé sur GitHub Pages** (mises à jour via
   `flatpak update`) : construire avec
   `flatpak-builder --repo=repo --gpg-sign=<CLÉ> ...`, puis
   `flatpak build-update-repo --generate-static-deltas --gpg-sign=<CLÉ> repo`,
   publier le dossier `repo/` sur la branche `gh-pages`, et fournir un fichier
   `dsone.flatpakrepo` (Url = `https://technomagniaque.github.io/dsone/repo/`,
   GPGKey = clé publique en base64). Les utilisateurs font une fois :
   `flatpak remote-add --user dsone https://technomagniaque.github.io/dsone/dsone.flatpakrepo`.
   (Non automatisé ici : nécessite une clé GPG dédiée en secret du dépôt.)
3. **Paquets natifs** : `.deb`, `.rpm` et paquet Arch générés par
   `.github/workflows/distro-builds.yml`.

## Fichiers

| Fichier | Rôle |
|---|---|
| `flatpak/io.github.technomagniaque.dsone.yml` | Manifeste « release » (source `git` + `tag`) : c'est lui qui va dans le dépôt Flathub. |
| `flatpak/io.github.technomagniaque.dsone.local.yml` | Même manifeste, source `dir` (la copie locale). Pour les builds locaux et la CI. À garder synchronisé. |
| `flatpak/cargo-sources.json`, `flatpak/pnpm-sources.json` | Sources hors ligne générées (non versionnées, cf. `.gitignore`). |
| `data/io.github.technomagniaque.dsone.metainfo.xml` | AppStream (validé par `appstreamcli validate`). |
| `data/io.github.technomagniaque.dsone.desktop` | Entrée de bureau (validée par `desktop-file-validate`). |
| `src-tauri/icons/icon.png` (512 px), `128x128@2x.png`, `128x128.png` | Icônes installées sous ce même identifiant. |

Générer les sources hors ligne (outil épinglé comme dans la CI) :

```sh
git clone https://github.com/flatpak/flatpak-builder-tools.git
git -C flatpak-builder-tools checkout ac5a296ac6111aa2319daf532f609a067b88d8a9
pip install tomlkit==0.14.0 aiohttp==3.13.3 ./flatpak-builder-tools/node
python3 flatpak-builder-tools/cargo/flatpak-cargo-generator.py \
  src-tauri/Cargo.lock -o flatpak/cargo-sources.json
flatpak-node-generator pnpm --pnpm-store-version v11 pnpm-lock.yaml \
  -o flatpak/pnpm-sources.json
```

Build local :

```sh
flatpak install --user flathub org.gnome.Sdk//50 org.gnome.Platform//50 \
  org.freedesktop.Sdk.Extension.rust-stable//25.08 \
  org.freedesktop.Sdk.Extension.node24//25.08
flatpak-builder --user --install --force-clean flatpak/build-dir \
  flatpak/io.github.technomagniaque.dsone.local.yml
flatpak run io.github.technomagniaque.dsone
# lint Flathub
flatpak run --command=flatpak-builder-lint org.flatpak.Builder manifest \
  flatpak/io.github.technomagniaque.dsone.yml
```

## Permissions (finish-args) et justification

| Permission | Pourquoi |
|---|---|
| `--socket=wayland`, `--socket=fallback-x11`, `--share=ipc` | Fenêtre GTK/WebKit. |
| `--share=network` | Streaming Deezer/TIDAL. |
| `--socket=pulseaudio` | Mode de sortie normal (PulseAudio / pipewire-pulse). |
| `--filesystem=xdg-run/pipewire-0` | Clients PipeWire natifs. Flatpak n'a pas de `--socket=pipewire`. |
| `--device=all` | Mode ALSA exclusif / bit-perfect : accès direct à `/dev/snd/*` (`hw:`), que `--socket=pulseaudio` n'ouvre pas. Couvre aussi DRI. À justifier dans la PR : c'est la fonction centrale de l'application. |
| `--talk-name=org.kde.StatusNotifierWatcher` | Icône de la zone de notification (ksni). |
| `--filesystem=xdg-run/discord-ipc-*` etc. | Discord Rich Presence, liste identique à upstream. |
| `--env=WEBKIT_DISABLE_COMPOSITING_MODE=1` | Repris d'upstream. |

- `--talk-name=org.freedesktop.Notifications` : notification de bureau (avec un
  bouton « Retry ») quand la sortie exclusive échoue et que la lecture repasse
  sur PipeWire (`src-tauri/src/audio_output.rs`).

Volontairement absents :
- `--own-name=org.mpris.MediaPlayer2.dsone` : le nom MPRIS est
  `org.mpris.MediaPlayer2.io.github.technomagniaque.dsone` (`src-tauri/src/mpris.rs`),
  que Flatpak autorise d'office ; le nom court `dsone` n'est utilisé que sous Snap.
- `--talk-name=org.freedesktop.secrets` : comme upstream ; la clé est alors
  stockée dans un fichier `0600` du bac à sable (`src-tauri/src/crypto.rs`).

GStreamer vient du runtime GNOME 50 (≥ 1.26.10, avec `curlhttpsrc`), comme
upstream : pas de module GStreamer à compiler.

## Étapes de soumission

Référence : <https://docs.flathub.org/docs/for-app-authors/submission>.

1. Pré-requis côté dépôt `technomagniaque/dsone` :
   - publier un tag `vX.Y.Z` et une Release ;
   - mettre à jour `<releases>` dans le metainfo (fait par `pnpm version`
     via `sync-version.mjs`) ;
   - remplacer les captures d'écran provisoires (celles de SONE) par des
     captures de dsone, accessibles en `https://raw.githubusercontent.com/technomagniaque/dsone/<tag>/data/...` ;
   - vérifier `appstreamcli validate --no-net` et `desktop-file-validate`.
2. Forker <https://github.com/flathub/flathub> **en décochant
   « Copy the master branch only »**, puis créer une branche à partir de
   `new-pr` :
   ```sh
   git clone --branch=new-pr git@github.com:technomagniaque/flathub.git
   cd flathub && git checkout -b io.github.technomagniaque.dsone
   ```
3. Y copier, à la racine :
   - `io.github.technomagniaque.dsone.yml` (manifeste release) avec `tag` **et**
     `commit` renseignés (`git rev-parse vX.Y.Z^{commit}`) ;
   - `cargo-sources.json` et `pnpm-sources.json` générés pour ce tag ;
   - optionnel : `flathub.json` (par ex. `{"only-arches": ["x86_64", "aarch64"]}`).
4. Commit, push, puis ouvrir une PR **vers la branche `new-pr`** de
   `flathub/flathub`, titre « Add io.github.technomagniaque.dsone ». Dans la
   description : fork de SONE (déjà sur Flathub), différences (client Deezer),
   justification de `--device=all`, et mention explicite du déchiffrement des
   flux Deezer via une API non officielle.
5. Répondre aux relecteurs ; un build de test se lance via le commentaire
   `bot, build`. Après acceptation, Flathub crée
   `flathub/io.github.technomagniaque.dsone` et vous y donne accès.
6. Activer les mises à jour automatiques : dans ce dépôt, définir la variable
   d'Actions `FLATHUB_REPO=flathub/io.github.technomagniaque.dsone` et le secret
   `FLATHUB_TOKEN` (jeton avec accès en écriture à ce dépôt). Le workflow
   `.github/workflows/flathub-update.yml` reste inerte tant que la variable
   n'a pas exactement cette valeur, et ne peut pas viser le dépôt Flathub de SONE.
7. Vérification de propriété : l'identifiant `io.github.technomagniaque.*`
   se vérifie sur flathub.org via le compte GitHub `technomagniaque`.

## Compatibilité des distributions

| Distribution | GStreamer (dépôts) | PipeWire | Recommandé |
|---|---|---|---|
| Debian 13 (trixie) | 1.26.2 | 1.4.x | Flatpak (ou `.deb`) |
| Kubuntu 26.04 LTS | 1.26.x / 1.28.x (à vérifier) | 1.4.x+ | `.deb` ou Flatpak |
| Fedora 44 | 1.26.x récent / 1.28.x (à vérifier) | 1.4.x+ | `.rpm` ou Flatpak |
| CachyOS (Arch) | dernière (rolling) | dernière | paquet `makepkg` |
| Flatpak (toutes) | runtime GNOME 50, ≥ 1.26.10 | socket de l'hôte | `.flatpak` |

Sous GStreamer 1.26.10, avec un proxy **authentifié** uniquement, les flux
HTTP progressifs sont refusés (`src-tauri/src/proxy.rs`, `CURL_SEEK_FIXED`) ;
le DASH (lossless/Hi-Res TIDAL) et l'usage sans proxy ne sont pas touchés.
Debian 13 est concernée ; le Flatpak ne l'est pas. Les versions « à vérifier »
n'ont pas pu être consultées depuis l'environnement de préparation
(`gst-inspect-1.0 --version` pour contrôler).
