# PLAN.md — Cleanator

> Author's working notes, in French. Public documentation lives in `README.md` and `docs/`.

Plan vivant, source de verite de l'avancement. Version du 2026-10-15, alignee sur le BRIEF
rev. 4.

## Etape 0. Inventaire du vrai disque (ouverte le 2026-10-15 | close le 2026-10-15)

Resultat visible attendu : `00_cadrage/2026-10-XX_inventaire_disque.md` — ou partent les
2 To de la machine de dev, par emplacement, avec les tailles mesurees ; ecarts entre le
tableau du BRIEF §6 et la realite notes.

- [x] Lancer la commande d'inventaire (deux passages, `00_cadrage/2026-10-15_inventaire_disque.md`)
- [x] Comparer au tableau §6 (`00_cadrage/2026-10-15_inventaire_analyse.md`, §4 : 13 ecarts)
- [x] Corriger le tableau §6 (BRIEF rev. 5, entree JOURNAL)
- [x] Validation des rev. 4 et 5 par Alexandre (2026-10-15)

## Etape 1. Regles v0 (`rules/*.toml`) (ouverte le 2026-10-15 | close le 2026-10-15, reste « avant release »)

Resultat visible attendu : un fichier TOML par categorie, chaque regle avec chemins,
niveau, pourquoi, regeneration, cout, source (documentation de l'outil, lue et datee).

- [x] Lire la documentation : pip, uv, npm, pnpm, cargo, conda, Docker (pruning, system df),
      Ollama (FAQ, CLI), Hugging Face (manage-cache), Playwright — lues le 2026-10-15
- [x] Lire la documentation : snap, flatpak (man), journald (man), apt-get (man), Python venv,
      Claude Code (claude-directory), Codex (config-advanced), VS Code (uninstall) — 2026-10-15
- [x] Lire la documentation : pipx, pytest, vLLM, nvm, npx, JetBrains, LM Studio (CLI),
      Aider, gio, tracker3-reset, Gemini CLI — 2026-10-15 (16 `to-verify` → 10)
- [ ] Avant release : les 10 regles encore `to-verify` (Claude Desktop ×2 — pas de doc
      publique, observe ; Cline/Roo/Kilo ; OpenCode ; images de VM ; __pycache__ ;
      miniatures ; caches navigateurs ; apport/coredump ; apps LLM locales)
- [x] Ecrire le schema TOML d'une regle (`docs/rules.md`, anglais)
- [x] Regles : caches de paquets (10), Docker (6), modeles (8) — 2026-10-15
- [x] Relecture du format par Alexandre sur package-caches : « ca me suffit » (2026-10-15) ;
      exigence d'interface notee dans `docs/rules.md` (deux profondeurs d'affichage)
- [x] Regles : environnements Python (5), builds (5), assistants IA (14), systeme (13) —
      61 regles au total, 2026-10-15
- [x] Regles restantes : sauvegardes, archives et images, VM, depots re-clonables, gros
      fichiers non classes (`never`), nvm, CUDA (`never`), apps LLM locales, sorties
      d'entrainement — 70 regles en dix fichiers (31 safe, 28 review, 11 never), 2026-10-15
- [x] Relecture par Alexandre de `assistants.toml` : « je ne vois rien de bizarre, pour moi
      c'est bon » (2026-10-15)
- [x] Relecture finale automatique : chaque regle a pourquoi / regeneration / cout ; aucune
      `safe` sans nettoyage, aucune `never` avec nettoyage, aucune `review` en `rm` sauf
      les venvs (disposables par conception) — 0 incoherence
- [ ] Avant release : sources `to-verify` ci-dessus (bloquant)

## Etape 2. Coeur Rust en ligne de commande (`cleanator-cli`) (ouverte le 2026-10-16 dans la nuit | close le 2026-10-16)

Resultat visible attendu : `cleanator-cli scan ~ --json` sort un arbre dont les totaux
collent a `du -sh` ; `cleanator-cli recommend` sort les recommandations ;
`cleanator-cli script` ecrit le script. Aucune interface.

- [x] Espace de travail Cargo : `crates/core`, `crates/cli`, `crates/app` (vide), `rules/`,
      `locales/`, `docs/` ; versions figees (`Cargo.lock`) ; `cargo test` vert
- [x] Scanner : parcours parallele (rayon), blocs alloues, dedoublonnage par inode, pas de
      symlink, pas de traversee de point de montage — **egal a `du -sx` a l'octet pres** sur
      trois arbres de test (2,5 Go, 1,6 Go, 0,8 Go ; 43 ms pour 2,5 Go)
- [x] Sondeurs : Docker (`system df -v --format json`, taille *unique*, conteneurs `Created`,
      montages bind), Ollama (`ollama ls`/`ps`, repli sur les manifestes si le serveur est
      arrete), Hugging Face (dossiers `models--`/`datasets--`), snap, journald, nvm, git
      (re-clonable), conda, pnpm, apt autoremove ; flatpak : non implemente (pas de
      listing en lecture seule)
- [x] Chargement des regles (embarquees dans le binaire + `--rules DIR`), resolution des
      variables d'environnement (R8), validation = les regles de `docs/rules.md`
- [x] Classifieur : path / pattern (glob, marqueur, `requires_sibling`, `exclude_under`,
      ancre projet) / signature / probe ; dormance (`mtime` ou `atime:bin/python`) ; projet
      actif → `review` ; en usage : processus, fichiers ouverts (`/proc`), montages Docker
- [x] Generateur de script (R11–R13) : deux blocs, pourquoi en commentaire, `df` avant/apres
- [x] Tests : 21 (un cas par niveau, projet actif/dormant, venv recent/dormant, modele hors
      magasin, en usage signale, lien physique, script par selection) ; `cargo clippy` propre
- [x] Test de bout en bout sur un home fictif de 3,3 Go : 13 `safe`, 11 `review`, `never`
      pour la VM Claude Desktop et les gros fichiers inconnus
- [x] Compile sur la vraie machine (12 s) ; scanner = `du -sx` a l'octet pres sur 636 Go,
      2,59 M de fichiers, 2,1 s
- [x] Deux passes reelles ; cinq defauts corriges (un proprietaire par chemin, usage precis,
      dormance des modeles, `skip_in_project`, exclusions) ; liste `review` relue par
      Alexandre : « c'est bon, on continue » (2026-10-16)
- [ ] Reprendre d'Eliot ce qui y a deja ete resolu cote Rust (structure, tests) — a lire
- [x] Sondeurs sur la vraie machine : Docker (15,5 Go de cache de build, 18 Go tensorcash,
      3 poa-banc, image <none>), snap (15 revisions), journald, git (3 re-clonables), HF
- [ ] A reporter en v1.1 : la date d'acces est fragile (un passage le 2026-09-02 a tout
      remis a zero : sauvegarde ou indexeur) — croiser avec mtime, avertir en `noatime`
- [ ] A reporter en v1.1 : regrouper les petits elements (595 `safe` dont ~400 `__pycache__`)

## Etape 3. Spike iced, puis maquette d'interface (ouverte le 2026-10-16)

Resultat visible attendu : d'abord le spike — treemap + liste a cocher en iced sur donnees
fictives, une journee, regarde par Alexandre (« ca me plait » ou retour a Tauri + React) ;
puis les ecrans (bandeau, treemap, recommandations, assistants, projets dormants, apercu
du script) en statique, valides visuellement ; `locales/` complets en `en` et `fr`.

- [x] Spike : iced 0.13.1, treemap au canvas, liste avec cases, theme clair/sombre, i18n —
      ecrit et compile le 2026-10-16, a juger a l'ecran
- [x] Decision d'Alexandre sur le spike : « pour un premier jet, c'est plutot pas mal »
      (2026-10-16, deux captures) — iced confirme
- [x] Direction visuelle : palette des categories validee par script (dataviz) dans les
      deux modes, cartes nettes, legende traduite — 2026-10-16
- [x] Deux profondeurs d'affichage (`docs/rules.md`) et regroupement des regles nombreuses
      — dans l'app reelle, pas en statique (le spike etant l'app) — 2026-10-16
- [x] Vue « assistants IA » (configuration / historique / caches par assistant) et vue
      « projets » (poids regenerable par projet, actif/dormant), BRIEF §1 — 2026-10-16
- [x] Ecran de chargement (compteurs reels : octets, fichiers, dossiers, secondes ; etape
      « mesure » puis « regles ») ; icone (treemap, une tuile rayee cochee) et `.desktop`
      — 2026-10-16
- [x] Validation visuelle des trois onglets (captures du 2026-10-16, puis Projets en pleine
      largeur et en sombre : « pour moi, c'est bon ») — etape 3 close

## Etape 4. Assemblage et release 0.1.0

Resultat visible attendu : `.deb`, AppImage et `.rpm` sur GitHub Releases, installes et
testes sur la machine de dev.

- [x] Branchement interface ↔ coeur (appels directs a `crates/core`, scan en tache de fond)
      — fait des le spike, 2026-10-16
- [x] Paquets : `packaging/build.sh` → `dist/` : `.deb` (cargo-deb), `.rpm`
      (cargo-generate-rpm), AppImage (appimagetool) ; metadonnees dans
      `crates/app/Cargo.toml` ; `.deb` installe et teste dans le cloud (Ubuntu 24.04) —
      2026-10-16
- [ ] Test d'installation des trois sur la machine de dev (le `.deb` au moins ; `.rpm`
      verifie par `rpm -qpi` si `rpm` est present)
- [ ] Depot `blackberrylabs/cleanator` (LICENSE MIT, README minimal) — push sur accord
- [ ] Tag `v0.1.0`, release, note de version

## Etape 5. README et lancement

Resultat visible attendu : README anglais avec captures, section « why not Baobab / ncdu /
BleachBit » verifiee, `CONTRIBUTING.md`, `docs/rules.md`.

- [ ] Reverifier les outils existants cites au BRIEF §4, plus `cleanr` (crates.io, v0.17,
      outil de nettoyage en Rust) et `clean-rs`
- [ ] Captures d'ecran reelles (treemap, assistants, script)
- [ ] README, CONTRIBUTING, docs
- [ ] Partage (a decider : Reddit r/linux, Hacker News, LinkedIn) — accord d'Alexandre
