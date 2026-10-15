# BRIEF.md — Cleanator

> Author's working notes, in French. Public documentation lives in `README.md` and `docs/`.

Cadrage, ecrit une fois. Revision 3 validee par Alexandre le 2026-10-15. Revision 4 du
2026-10-15 (audit « multitude d'assistants IA » : §1 vue assistants, R6 etendu, R10 bis,
tableau §6, §8) et revision 5 du 2026-10-15 (inventaire du vrai disque,
`00_cadrage/2026-10-15_inventaire_analyse.md` : detection par signature, venvs partout,
`target/` actifs, montages Docker, sauvegardes, Claude Desktop, re-clonables) : validees le
2026-10-15. Revision 6 du 2026-10-15 : pile 100 % Rust (iced) a la place de Tauri + React,
tranchee par Alexandre apres examen de Sniffnet, Halloy et COSMIC (§6, R14, R16, R17, §10). Toute modification passe par une entree du JOURNAL.

## 1. Objet

Cleanator est une application de bureau Linux qui montre ou part le disque d'un developpeur
qui fait de l'IA, et lui propose quoi liberer — sans jamais rien supprimer elle-meme.

Trois vues dans une seule fenetre, calquees sur la maquette de reference
(`00_cadrage/references/2026-10-15_maquette_treemap_recommandations.png`) :

- un bandeau de quatre compteurs : libre apres nettoyage / sur a supprimer / a verifier /
  non analyse ;
- un treemap du dossier personnel (puis du disque), colore par categorie : projets, caches de
  paquets, modeles IA, environnements Python, Docker, outils IA, medias, systeme ; les
  modeles y apparaissent par nom (`llama3.1:8b`, `mistral-7b-instruct`), jamais par hash ;
- une liste de recommandations a cocher, classee par niveau de securite, qui se termine par
  un bouton « generer le script » : un script shell lisible, commente ligne a ligne, que
  l'utilisateur lit puis lance lui-meme.

Une quatrieme vue : les assistants IA, un par un (Claude Code, Codex, Gemini CLI, Cursor,
VS Code et ses extensions d'agents, Windsurf, Continue, Zed, OpenCode, Aider, JetBrains…),
chacun decoupe en trois parts — configuration (`never`), historique de sessions (`review`
au-dela du seuil de dormance, avec la mention de ce qui en depend, par exemple
`claude --resume`), caches/index/logs (`safe`). « Claude Code 2,1 Go dont 1,8 Go de sessions
d'avant juillet » : c'est la tuile qui parle au public vise.

Une cinquieme vue decoule des autres : la liste des projets dormants (depot git ou
`package.json`/`pyproject.toml` sans modification depuis N jours) avec leur poids regenerable
(`node_modules`, `.venv`, `target`, `.next`) — « 12 projets dormants, 38 Go regenerables ».

Nom public : Cleanator. Depot : `github.com/blackberrylabs/cleanator`. Licence : MIT.

## 2. Pourquoi ce projet (pour moi)

- Besoin reel : 2 To de NVMe qui se remplissent de modeles, de caches, d'images Docker, de
  `target/` Rust et de `node_modules` de projets dormants, sans outil qui sache lesquels sont
  regenerables.
- Vitrine open source : premier depot de l'organisation Blackberrylabs, avec Eliot ensuite.
  Objectif = audience et reconnaissance (etoiles, partages, retours), pas revenu.
- Matiere a contenu : un treemap qui montre « 60 Go de modeles non charges depuis juin » fait
  une capture et une video courte.
- Preuve que la methode (cadrage ecrit, regles sourcees, un vibe codeur qui livre propre)
  tient sur un outil public.

## 3. Public et posture

- Public : le developpeur sous Linux qui utilise l'IA au quotidien — LLM locaux (Ollama,
  vLLM, llama.cpp, LM Studio, Hugging Face), generation d'images (ComfyUI, Stable
  Diffusion), Docker avec images CUDA, Python (uv, pip, conda), Node, Rust, outils d'agents
  (Claude Code, Cursor, OpenCode, Copilot, Aider). Ubuntu/Debian d'abord.
- Posture : l'outil explique et propose, l'utilisateur decide et execute. Chaque
  recommandation dit pourquoi c'est supprimable, comment ca se regenere, et combien ca
  coutera a regenerer (un cache npm : secondes ; un modele de 15 Go : un telechargement).
- Langue : interface en anglais, francais en option (detection de la langue du systeme,
  bascule dans les reglages). README, code, commentaires, commits, issues : anglais.
- Simple, facile, intuitif (principe de produit, dit par Alexandre le 2026-10-15) : une
  fenetre ; le scan demarre a l'ouverture sans reglage ; aucune notion a apprendre avant le
  premier resultat ; trois gestes — regarder, cocher, copier le script. Chaque ecran ou
  reglage ajoute doit se justifier contre ce principe.
- Entierement gratuit, pas de freemium (tranche le 2026-10-15) : en MIT une partie payante
  ne se protege pas, les fonctions qui rendent l'outil juste sont le coeur et ne se
  separent pas, et un paywall est de la friction. Possible plus tard, sans toucher a ca :
  GitHub Sponsors ; une edition « equipe » separee si une entreprise le demande, decidee
  seulement avec une audience etablie.

## 4. Contexte verifie le 2026-10-15 (sources datees)

- Luno (app.lunomac.com, macOS 14+, v1.3.0) : cinq outils — Nettoyer (11 categories de caches
  triees par securite), Desinstaller, Optimiser (22 taches), Analyser (treemap, clic droit
  vers la Corbeille), Statut (grille CPU/memoire/GPU/disque/reseau avec sparklines). Vu le
  2026-10-15, captures dans `00_cadrage/references/`. Cleanator reprend Analyser + Nettoyer.
- Maquette de reference (capture du 2026-10-15 14:42, origine non identifiee) : bandeau de
  compteurs, treemap par categorie, recommandations « safe to remove / review first »,
  bouton « Copy script ». C'est le modele de la v1.
- Nom : `github.com/cleanator` libre (404 le 2026-10-15). Quatre depots homonymes chez des
  particuliers, 0 etoile chacun, aucun n'est un nettoyeur de disque
  (api.github.com/search/repositories?q=cleanator+in:name, 2026-10-15).
- Organisation `blackberrylabs` creee le 2026-10-15 (api.github.com/orgs/blackberrylabs),
  0 depot.
- Outils existants sous Linux, connus et non reverifies ce jour : Baobab (analyseur GNOME,
  montre sans classer), ncdu (terminal), BleachBit (nettoie sans connaitre un poste de dev),
  Stacer. Aucun ne connait les emplacements IA. A reverifier avant le README (section
  « pourquoi pas X »).
- Comportements d'outils a verifier dans leur documentation avant d'ecrire la regle
  (tache 1) : Ollama (blobs partages + manifestes, `ollama rm`), Hugging Face (`hub/` en
  `blobs/` + `snapshots/` lies, `huggingface-cli delete-cache`), uv et pnpm (liens
  physiques entre cache et environnements), Docker (`docker system df -v` sans root,
  `prune`), snap (`snap list --all`, `snap remove --revision`), flatpak
  (`flatpak uninstall --unused`), journald (`journalctl --vacuum-size`).
- Machine de developpement : Ubuntu 24.04, 2 To NVMe, RTX 3090 (`00_moi/STACK.md`).

## 5. Regles du projet (au-dela de CONVENTIONS.md)

Securite et verite des chiffres :

- R1 — Jamais de suppression par l'outil. Cleanator lit le disque et ecrit un script ; il ne
  lance rien et n'efface rien. Regle non negociable, valable pour toutes les versions.
- R2 — Trois niveaux et un seul par defaut : `safe` (regenerable sans perte : caches,
  builds), `review` (recuperable mais couteux ou ambigu : modeles, volumes Docker,
  `node_modules` d'un projet inactif, sorties d'entrainement), `never` (tout le reste). Ce
  que l'outil ne connait pas est `never` et n'est jamais propose.
- R3 — Chaque regle de classement est declarative, dans un fichier de regles du depot
  (`rules/*.toml`) : chemins, categorie, niveau, pourquoi, comment ca se regenere, cout de
  regeneration, source (documentation de l'outil concerne), date. Publiee, donc contestable
  par issue.
- R4 — Un magasin gere par un outil se nettoie par la commande de cet outil, jamais par
  `rm` : Ollama (`ollama rm`), Docker (`prune`), pnpm (`pnpm store prune`), uv
  (`uv cache clean`), snap, flatpak, apt, journald. `rm` n'apparait que pour un dossier
  simple sans index (un `node_modules`, un `target/`, un cache de navigateur).
- R5 — Les tailles sont vraies : blocs alloues (pas taille apparente), liens physiques
  comptes une fois par inode, et la recommandation dit « partage avec N environnements »
  quand vider un cache ne liberera pas sa taille affichee (uv, pnpm). Sur btrfs/ZFS avec
  instantanes, l'outil previent que l'espace libere peut n'apparaitre qu'apres suppression
  des instantanes.
- R6 — Jamais proposer ce qui est en usage : modele charge dans Ollama ou vLLM (processus
  et API locale consultes), image d'un conteneur en cours, environnement d'un projet
  modifie recemment, sessions et caches d'un assistant dont le processus tourne (`claude`,
  `codex`, `cursor`, `code`, `windsurf`, `zed`…), et tout chemin monte (bind mount) dans un
  conteneur en cours d'execution (`docker inspect`) — les venvs des plugins Dify sous
  `docker/volumes/` en sont l'exemple vecu.
- R7 — Les sorties d'entrainement et de generation (`checkpoints/`, `wandb/`, `runs/`,
  `outputs/`, `lightning_logs/`, fichiers `.safetensors`/`.ckpt`/`.pt` hors magasin) sont
  `review` avec l'avertissement « peut etre votre seule copie », jamais `safe`.
- R8 — Les chemins viennent des variables d'environnement avant les defauts : `XDG_*`,
  `OLLAMA_MODELS`, `HF_HOME`/`HF_HUB_CACHE`, `UV_CACHE_DIR`, `PIP_CACHE_DIR`, `CARGO_HOME`,
  `npm_config_cache`, `PNPM_HOME`, `GOMODCACHE`, `data-root` de Docker. Un dev IA deplace
  souvent ses modeles sur un second disque.
- R9 — Analyse en lecture seule : ne suit pas les liens symboliques, ne traverse pas les
  points de montage sans le demander, ignore `/proc`, `/sys`, `/dev`, `/run`. Ce qui n'est
  pas lisible sans root est mesure par la commande de l'outil (`docker system df -v`) ou
  affiche comme « non analyse », jamais devine.
- R10 — Zero reseau : pas de telemetrie, pas d'appel sortant, pas de verification de mise a
  jour en v1. Le binaire ne parle a personne.
- R10 bis — Metadonnees seulement : Cleanator ne lit jamais le contenu d'un fichier (nom,
  taille, dates, inode, et les manifestes/index des magasins geres suffisent). Les fichiers
  d'identification et de configuration des assistants (`auth.json`, `credentials*`, `.env`,
  cles API, `settings.json`, `CLAUDE.md`, `AGENTS.md`, `.cursor/rules`) sont `never`, jamais
  cites dans le script, jamais affiches en detail.

Script genere :

- R11 — Lisible par un humain : un bloc par element, son pourquoi en commentaire,
  `set -euo pipefail`, chaque chemin entre guillemets, jamais de motif joker (`*`) construit
  a l'execution, et un `du` avant/apres pour que l'utilisateur voie ce qu'il a gagne.
- R12 — Deux blocs separes et annonces : commandes utilisateur, puis commandes `sudo`
  (apt, journald, snap, Docker si l'utilisateur n'est pas dans le groupe `docker`).
  L'utilisateur peut ne lancer que le premier.
- R13 — `safe` : `rm -rf` (ou la commande de l'outil, R4), espace libere tout de suite.
  `review` : deplacement vers la corbeille (`gio trash`), recuperable. Tranche le 2026-10-15.

Depot et publication :

- R14 — Aucune chaine de caracteres en dur dans l'interface : tout passe par les fichiers
  de `locales/` (`en` et `fr`, format de la bibliotheque i18n retenue au spike), des le
  premier ecran.
- R15 — Derogation a CONVENTIONS.md (langue) : code, commentaires, commits, issues et README
  en anglais, parce que le depot est public et ouvert aux contributions. Documentation
  publique en anglais seulement, jamais traduite (une doc en deux langues est fausse au
  bout de trois mois). Les quatre fichiers de pilotage (`BRIEF`, `PLAN`, `JOURNAL`,
  `CLAUDE`) restent en francais, a la racine, chacun ouvert par une ligne anglaise
  « Author's working notes, in French… » : c'est l'atelier de l'auteur, pas la doc.
- R16 — Derogation a CONVENTIONS.md (arborescence) : l'arborescence est celle d'un
  espace de travail Cargo (`Cargo.toml` racine, `crates/core`, `crates/cli`, `crates/app`,
  `rules/`, `locales/`, `docs/`), pas des dossiers numerotes, pour que tout dev Rust s'y
  retrouve. Seul `00_cadrage/` garde la numerotation.
- R17 — Une release = un tag + les trois paquets (`.deb` par `cargo-deb`, `.rpm` par
  `cargo-generate-rpm`, AppImage par `appimagetool`/`linuxdeploy`) sur GitHub Releases +
  une note de version. Le binaire est statique vis-a-vis du bureau : aucune dependance a
  une webview. Push et release : accord explicite (AUTONOMIE.md).

## 6. Architecture envisagee

Pile : 100 % Rust. Interface avec `iced` (pur Rust, MIT, toolkit du bureau COSMIC),
coeur en crate separee, binaire CLI. Tranche le 2026-10-15 (rev. 6) : la contrainte qui
imposait Tauri + React a Eliot (editeur graphique, canvas web) n'existe pas ici ; l'interface
de Cleanator est petite (compteurs, treemap, liste a cocher, apercu de script) ; un binaire
sans webview evite la dependance a `webkit2gtk`, source classique d'AppImage Tauri qui ne
demarrent pas d'une distribution a l'autre ; et « written in Rust, no webview » parle au
public vise. Garde-fou : un spike d'une journee a l'etape 3 (treemap + liste en iced sur
donnees fictives), valide visuellement par Alexandre avant tout branchement ; en cas
d'echec, retour a Tauri + React sans toucher au coeur. Version d'iced figee au spike.

Coeur Rust (`crates/core`), testable sans interface (`crates/cli` → `cleanator-cli`, sortie
JSON) :

- `scanner` : parcours parallele du dossier personnel (puis d'un chemin au choix), tailles
  en blocs alloues, dedoublonnage par inode (R5), resultat en arbre ; cache du dernier scan
  dans `~/.cache/cleanator/` pour rouvrir vite ;
- `probes` : un sondeur par outil qui sait lire son magasin et son etat — Ollama
  (manifestes → noms de modeles, API locale → modeles charges), Hugging Face (dossiers
  `models--org--nom`, `datasets--`), Docker (`system df -v`, conteneurs actifs), uv/pnpm
  (liens physiques), snap/flatpak (revisions et runtimes inutilises), journald ;
- `rules` : chargement des `rules/*.toml` ; un detecteur par famille (chemin fixe, motif
  `**/node_modules`, commande, metadonnee comme la date de dernier acces) ; resolution des
  variables d'environnement (R8) ;
- `classifier` : applique regles et sondeurs a l'arbre, exclut ce qui est en usage (R6),
  produit les recommandations avec niveau, taille vraie, pourquoi, regeneration, cout ;
- `script` : ecrit le script shell selon R11–R13.

Interface iced (`crates/app`) :

- bandeau de compteurs ; treemap (algorithme squarified, widget `canvas`, clic pour
  descendre, fil d'Ariane) ; panneau de recommandations avec cases a cocher et total
  selectionne ; vue « assistants IA » ; vue « projets dormants » ; apercu du script avant
  copie/enregistrement ;
- rendu natif propre, reference visuelle Sniffnet (cartes nettes, couleurs par categorie),
  pas un clone de la maquette web ; theme clair par defaut, theme sombre suivant le
  systeme ;
- i18n `en`/`fr` (R14).

Detection par nature avant detection par chemin (inventaire du 2026-10-15 : 100 Go de
modeles et 48 Go de venvs hors de tout chemin standard) :

- un modele se reconnait a sa signature — `.safetensors`, `.gguf`, `.ckpt`, `.pt`/`.pth`,
  `.onnx`, `.bin` au-dela de 500 Mo — ou qu'il soit, puis se rattache a son magasin s'il en
  a un (manifeste Ollama, dossier `models--` HF, `models/` de ComfyUI) ;
- un environnement Python se reconnait a `pyvenv.cfg` (ou `conda-meta/`), ou qu'il soit ;
- un build se reconnait a son dossier (`target/` avec `CACHEDIR.TAG`, `node_modules/`,
  `.next/`, `dist/`) rattache au projet parent, et le projet a son activite (dernier
  fichier modifie hors builds) ;
- une sauvegarde se reconnait a son nom (`backup`, `bak`, `_BACKUP_`, date dans le nom,
  `.tar.gz`/`.zip` a la racine du home) ;
- un depot git re-clonable se reconnait a : arbre propre, remote distant, rien de non
  pousse.

Categories et emplacements de depart (confirmes et corriges par l'inventaire du
2026-10-15 ; defauts Linux, variables d'environnement prioritaires) :

| Categorie | Emplacements | Niveau | Nettoyage |
|---|---|---|---|
| Modeles LLM | `~/.ollama/models` (par nom via manifestes ; aussi le volume Docker `ollama`), `~/.cache/huggingface/hub` et l'ancienne disposition `~/.cache/huggingface/models--*`, `~/.cache/lm-studio/models`, tout `.safetensors`/`.gguf` hors magasin | review, par nom, avec la date de dernier acces | `ollama rm`, `huggingface-cli delete-cache`, corbeille |
| Modeles image/audio | ComfyUI / Stable Diffusion / ai-toolkit `models/`, `~/.cache/whisper`, `~/.cache/torch/hub`, `~/.cache/torch/sentence_transformers` | review | corbeille |
| Datasets | `~/.cache/huggingface/datasets` | review | corbeille |
| Sorties d'entrainement | `checkpoints/`, `wandb/`, `runs/`, `outputs/`, `lightning_logs/` dans un projet | review + avertissement (R7) | corbeille |
| Environnements Python | tout dossier avec `pyvenv.cfg`, ou qu'il soit (`~/venvs/`, `~/venv-*`, `.venv` de projet ; torch + CUDA : 8 a 11 Go chacun), envs conda (`~/miniconda3/envs`), outils `uv tool` et pipx | review si inactif 60 j (date d'acces de `bin/python`) | `rm` / `conda env remove` / `uv tool uninstall` |
| Caches Python | `~/.cache/uv` (liens physiques, R5), `~/.cache/pip`, conda `pkgs`, `~/.cache/pypoetry` | safe | `uv cache clean`, `pip cache purge`, `conda clean --all` |
| Caches Node | `~/.npm/_cacache`, store pnpm (liens physiques), `~/.cache/yarn`, `~/.bun/install/cache` | safe | commande de l'outil |
| Navigateurs de test | `~/.cache/ms-playwright`, `~/.cache/puppeteer` | safe | `rm` |
| Caches Rust / Go | `~/.cargo/registry`, `~/.cargo/git`, `~/go/pkg/mod`, `~/.cache/go-build` | safe | `rm` |
| Builds de projets | `**/node_modules`, `**/target` (Rust ; 26 Go sur un seul projet Tauri), `**/.next`, `**/dist`, `**/.turbo`, `**/__pycache__`, `**/.pytest_cache` | safe si projet dormant ; review avec cout de reconstruction si projet actif | `rm` / `cargo clean` |
| Docker / Podman | cache de build et images `<none>` (safe) ; images taguees sans conteneur, images d'un conteneur `Created` jamais demarre, volumes sans conteneur (review) ; `~/.local/share/containers` — taille affichee = colonne *unique/reclaimable* de `docker system df -v`, jamais la taille brute (couches partagees) | safe / review | `docker builder prune`, `docker image prune`, `docker image rm <tag>`, `docker volume rm` |
| Assistants IA — historique | Claude Code `~/.claude/projects` (JSONL, base de `--resume`), `~/.claude/worktrees` ; Claude Desktop `~/.config/Claude/local-agent-mode-sessions/*/audit.jsonl` (1 Go par session longue) ; Codex `~/.codex/sessions` ; Gemini CLI `~/.gemini` ; Cline/Roo/Kilo `globalStorage/<ext>/tasks` (checkpoints, des Go) ; Goose, OpenCode (`~/.local/share/opencode`, `opencode.db.bak`), Continue sessions ; `.aider.chat.history.md` | review, au-dela de 60 jours | corbeille |
| Assistants IA — en usage | Claude Desktop `~/.config/Claude/vm_bundles/` (VM Cowork : `rootfs.img`, `sessiondata.img`, 20 Go, modifies chaque jour) | never (R6) | — |
| Assistants IA — caches | Cursor et VS Code `~/.config/<App>/{Cache,CachedData,CachedExtensionVSIXs,logs,workspaceStorage}`, `~/.cursor/extensions` et `~/.vscode/extensions` non actives, `~/.codeium`, `~/.continue/index`, `~/.cache/opencode`, `~/.cache/zed`, `~/.cache/JetBrains`, `.aider.tags.cache.*` | safe | `rm` |
| Assistants IA — configuration | `~/.claude.json`, `~/.claude/settings.json`, `~/.codex/auth.json`, `~/.config/<App>/User/settings.json`, `CLAUDE.md`, `AGENTS.md`, `.cursor/rules`, `.env` | never (R10 bis) | — |
| Serveurs MCP et outils CLI | `~/.npm/_npx` (copies laissees par chaque `npx` de serveur MCP), outils `uvx` dans `~/.cache/uv`, `~/.local/share/uv/tools`, `~/.local/pipx/venvs` | safe (npx) / review (outils installes) | `rm` / `uv tool uninstall`, `pipx uninstall` |
| Versions de Node multiples | `~/.nvm/versions/node/*` hors version active (chacune avec ses CLI globaux `claude`, `codex`, `gemini`) | review | `nvm uninstall` |
| LLM locaux des assistants | `~/.lmstudio`, Jan `~/jan/models`, GPT4All `~/.local/share/nomic.ai`, volumes Docker d'Open WebUI | review | corbeille / `docker volume rm` |
| Toolchains GPU | `/usr/local/cuda-*` multiples, `~/.nv/ComputeCache` | never (signale seulement) / safe | — / `rm` |
| Navigateurs | `~/.cache/mozilla`, `~/.cache/google-chrome`, `~/.cache/chromium`, `~/.cache/BraveSoftware` | safe | `rm` |
| Index et temporaires | `~/.cache/tracker3` (index GNOME, 3,6 Go vu), `/tmp/pytest-of-*`, `~/.cache/vllm`, `~/.cache/torch_compile`, `~/.cache/electron`, `~/.cache/node-gyp` | safe | `tracker3 reset --filesystem`, `rm` |
| Sauvegardes et archives | `*_backup*`, `*_BACKUP_*`, `*.bak`, `*.tar.gz`/`*.zip` a la racine du home ou a cote de l'original (`~/.config/Claude_backup` 23 Go, `claude_config_backup_*.tar.gz`, `opencode.db.bak`) | review, avec la date et l'original s'il existe encore | corbeille |
| Depots git re-clonables | depot a la racine du home ou hors `Bureau/Projets`, arbre propre, remote distant, rien de non pousse (`claude-desktop-debian` 6 Go, `openclaw-src`, `llama.cpp`) | review | corbeille |
| Systeme | `/var/cache/apt`, journald, revisions snap inactives, runtimes flatpak inutilises, noyaux anciens, `/var/crash`, `~/.cache/thumbnails`, corbeille | safe | commande de l'outil (bloc sudo, R12) |
| Gros fichiers isoles | tout fichier > 1 Go hors magasin et hors categorie ci-dessus : `.iso`, `.img`, `.img.xz`, `.tar.gz`, `.deb`, `.AppImage`, partout dans le home (vu : image HiveOS 13 Go de 2023, ISO 2,7 Go) ; images de VM (`~/.local/share/libvirt/images`, VirtualBox, `~/.vagrant.d/boxes`) | review, avec la date | corbeille |
| Instantanes | Timeshift, snapshots btrfs | never (signale seulement) | — |
| Tout le reste | — | never | — |

## 7. Premieres taches (apres validation du BRIEF)

0. Inventaire du vrai disque : une commande `du` copiable, resultat dans
   `00_cadrage/2026-10-XX_inventaire_disque.md` — les regles de depart viennent de la, pas
   de tete.
1. `rules/` v0 : les emplacements du tableau, un fichier TOML par categorie, chacun avec son
   pourquoi, son cout de regeneration et sa source (documentation de l'outil, lue, datee).
2. Coeur Rust en ligne de commande : scan (inodes dedoublonnes) + sondeurs Ollama, Hugging
   Face, Docker, uv + classification + script, sortie JSON, testee sur la machine de dev.
   Aucune interface avant que ce coeur soit juste et que ses chiffres collent a `du`.
3. Maquette d'interface (statique) validee visuellement avant branchement.
4. Assemblage Tauri, premiere release `0.1.0` : `.deb` + AppImage + `.rpm`.
5. README anglais avec captures, section « why not Baobab / ncdu / BleachBit » verifiee.

## 8. Hors perimetre

- v1 : Desinstaller, Optimiser, Statut (Statut = candidat v2 si envie), Flathub (v2 si
  audience), detection des doublons (meme modele dans deux magasins, N versions de Node, N Chromium —
  Playwright, Puppeteer, Electron — candidat v2 et premiere fonction de la v2 : ~60 Go de
  doublons trouves a la main sur la machine de dev le 2026-10-15), Windows et macOS, suppression directe par l'outil, scan planifie, mise a jour
  automatique, telemetrie, toute version payante, nettoyage de `/var/lib/docker` autrement
  que par les commandes Docker, toute action sur les toolchains CUDA et les instantanes.

## 9. Regime : cadrage avant code

Rien n'est code avant la validation de ce BRIEF ; aucune mise en ligne avant que la
conception (regles v0, coeur CLI, maquette) soit finie.

## 10. Questions tranchees (datees) et questions ouvertes

Tranchees le 2026-10-15 (conversation Cowork, entree JOURNAL du meme jour) :

- Perimetre v1 = Analyser + Nettoyer fusionnes ; Statut plus tard ; Desinstaller et
  Optimiser abandonnes.
- Public = developpeur Linux qui fait de l'IA ; c'est le coeur du produit.
- Open source, MIT, publie sous l'organisation GitHub Blackberrylabs (creee le 2026-10-15).
- Interface anglais + francais en option ; code, commentaires, commits en anglais.
- Pile : Tauri + React + Rust (tranche le 2026-10-15, puis remplace le meme jour, rev. 6)
  → **100 % Rust, interface iced, avec spike a l'etape 3** (« je dis oui », apres examen
  de Sniffnet, Halloy, COSMIC).
- Installation : `.deb` + AppImage + `.rpm` (R17), sur GitHub Releases.
- Nom Cleanator, verifie libre sur GitHub.
- Q2 (script) : `rm` pour `safe`, corbeille pour `review` (R13).
- Q4 (perimetre d'analyse) : dossier personnel + emplacements systeme connus (`/var/cache/apt`,
  journald, snap, flatpak, Docker par sa propre commande) ; disque entier en option, avec
  avertissement que sans root une partie reste « non analysee ».
- Modele economique : entierement gratuit, pas de freemium (§3).
- Q1 (pilotage) : les quatre fichiers en francais a la racine, en-tete anglais (R15).
  Documentation publique en anglais seulement, non traduite.
- Q3 (dormance) : 60 jours sans modification pour un projet (`node_modules`, `.venv`,
  `target`), 60 jours sans chargement pour un modele ; reglable dans l'interface.
- Q5 (domicile) : `Bureau/Logiciels/` est le domicile de tous les logiciels de bureau
  (Cleanator, Eliot) ; fiche a ajouter dans `00_moi/PROJETS.md` a la validation du BRIEF.
- Q6 (squelette) : `create-tauri-app` etait le choix de Claude ; remplace en rev. 6 par un
  espace de travail Cargo cree a la main (`cargo new`), trois crates (R16). Ce qu'Eliot a
  resolu cote Rust (structure du coeur, tests) reste a reprendre apres lecture de son depot.

Validation : revision 3 validee par Alexandre le 2026-10-15 (« Ok, je valide »). Revisions 4
(audit assistants IA) et 5 (inventaire du vrai disque) validees par Alexandre le 2026-10-15
(« Je valide »). Le BRIEF est clos ; toute modification passe par le JOURNAL.
