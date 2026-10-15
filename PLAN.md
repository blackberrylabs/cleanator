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

## Etape 2. Coeur Rust en ligne de commande (`cleanator-cli`) (a ouvrir sur le « go » d'Alexandre)

Resultat visible attendu : `cleanator-cli scan ~ --json` sort un arbre dont les totaux
collent a `du -sh` ; `cleanator-cli recommend` sort les recommandations ;
`cleanator-cli script` ecrit le script. Aucune interface.

- [ ] Espace de travail Cargo : `crates/core`, `crates/cli`, `crates/app` (vide), `rules/`,
      `locales/`, `docs/` ; versions figees ; `cargo test` vert sur un coeur vide
- [ ] Scanner : parcours parallele, blocs alloues, dedoublonnage par inode, pas de symlink,
      pas de traversee de point de montage
- [ ] Sondeurs : Ollama, Hugging Face, Docker, uv/pnpm, snap/flatpak, journald, assistants
- [ ] Chargement des regles, resolution des variables d'environnement (R8)
- [ ] Classifieur + exclusion de ce qui est en usage (R6)
- [ ] Generateur de script (R11–R13), deux blocs
- [ ] Tests : totaux contre `du`, un cas par niveau, un cas « en usage », un cas lien
      physique
- [ ] Reprendre d'Eliot ce qui y a deja ete resolu cote Rust (structure du coeur, tests)

## Etape 3. Spike iced, puis maquette d'interface

Resultat visible attendu : d'abord le spike — treemap + liste a cocher en iced sur donnees
fictives, une journee, regarde par Alexandre (« ca me plait » ou retour a Tauri + React) ;
puis les ecrans (bandeau, treemap, recommandations, assistants, projets dormants, apercu
du script) en statique, valides visuellement ; `locales/` complets en `en` et `fr`.

- [ ] Spike : version d'iced figee, treemap au canvas, liste avec cases, theme clair/sombre
- [ ] Decision d'Alexandre sur le spike (entree JOURNAL) — si non : retour a Tauri + React,
      le coeur ne change pas
- [ ] Direction visuelle (reference Sniffnet : cartes nettes, couleurs par categorie) et
      palette des categories
- [ ] Ecrans statiques sur donnees fictives, deux profondeurs d'affichage (`docs/rules.md`)
- [ ] Validation visuelle

## Etape 4. Assemblage et release 0.1.0

Resultat visible attendu : `.deb`, AppImage et `.rpm` sur GitHub Releases, installes et
testes sur la machine de dev.

- [ ] Branchement interface ↔ coeur (appels directs a `crates/core`, scan en tache de fond)
- [ ] Paquets : `cargo-deb`, `cargo-generate-rpm`, AppImage (`linuxdeploy`/`appimagetool`) ;
      test d'installation des trois sur la machine de dev
- [ ] Depot `blackberrylabs/cleanator` (LICENSE MIT, README minimal) — push sur accord
- [ ] Tag `v0.1.0`, release, note de version

## Etape 5. README et lancement

Resultat visible attendu : README anglais avec captures, section « why not Baobab / ncdu /
BleachBit » verifiee, `CONTRIBUTING.md`, `docs/rules.md`.

- [ ] Reverifier les outils existants cites au BRIEF §4
- [ ] Captures d'ecran reelles (treemap, assistants, script)
- [ ] README, CONTRIBUTING, docs
- [ ] Partage (a decider : Reddit r/linux, Hacker News, LinkedIn) — accord d'Alexandre
