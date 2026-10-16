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
- [x] Test d'installation du `.deb` sur la machine de dev : installe, icone en barre de
      titre, lancement depuis `/usr/bin` (capture du 2026-10-16 09:40) — `.rpm` non teste
      (pas de machine Fedora), AppImage extraite et verifiee dans le cloud
- [x] Sources des regles : les 10 `to-verify` lues et citees (freedesktop, Chromium, man
      systemd-coredump, Vagrant, Jan, GPT4All, Cline, OpenCode, Python) ; schema `observed`
      pour les deux regles Claude Desktop sans documentation publique ; test
      « zero to-verify » verrouille — 2026-10-16
- [x] Adresse de commit : noreply GitHub choisie par Alexandre, 11 commits reecrits
      (`git rebase --root --exec 'commit --amend --author'`), dates conservees — 2026-10-16
- [x] Depot `blackberrylabs/cleanator` cree par Alexandre, 11 commits pousses en SSH sur son
      accord — 2026-10-16 ; verifie en ligne : ni `00_cadrage/` ni `dist/`, MIT detectee
- [x] Tag `v0.1.0` pousse, release GitHub publiee par Alexandre avec les 4 fichiers de
      `dist/` (marquee Latest) ; `.deb` retelecharge anonymement et verifie (somme OK,
      `cleanator-cli 0.1.0`) — 2026-10-16. **Etape 4 close.**

## Etape 5. README et lancement (close le 2026-10-16)

Resultat visible attendu : README anglais avec captures, section « why not Baobab / ncdu /
BleachBit » verifiee, `CONTRIBUTING.md`, `docs/rules.md`.

- [x] Outils existants reverifies le 2026-10-16 : BleachBit 4.4.2 (paquet Ubuntu 24.04,
      75 nettoyeurs listes depuis le .deb, aucun pour VS Code/Docker/pip/npm/conda/uv/
      Ollama/HF), kondo 0.9 (artefacts de build, « rm -rf with a prompt »), cleanr 0.1.0
      (TUI caches dev, sans README), clean-rs (outil de clean de projets Rust/Go/Maven/
      Gradle, hors sujet) — tableau dans le README
- [x] Captures d'ecran reelles en anglais (explorateur clair et sombre, assistants,
      recommandation depliee), recadrees sur la fenetre, `docs/screenshots/` — 2026-10-16
- [x] README (captures, comparatif verifie, section Rust), CONTRIBUTING, docs/rules.md,
      CHANGELOG, About GitHub (description, 14 topics) — 2026-10-16
- [→] Partage (texte de presentation r/rust, r/linux, Show HN) : reporte apres la 0.1.1,
      decision d'Alexandre du 2026-10-16 — voir etape 6

## Etape 6. Version 0.1.1 — dormance robuste et usage reel (ouverte le 2026-10-16)

Resultat visible attendu : release 0.1.1 ; aucune recommandation fondee sur une date
d'acces non fiable ; `SHA256SUMS` propre ; ecran de chargement vu ; Alexandre a nettoye son
disque avec l'outil et le resultat est consigne. Perimetre valide par Alexandre le
2026-10-16 (« une 0.1.1 courte ») ; `.rpm` Fedora, commits non pousses et doublons vont en 0.2.

### Conception de la dormance (a valider par Alexandre avant le code)

Constat. Sept regles de modeles et quatre d'environnements Python mesurent le dernier usage
par la date d'acces (atime). Deux cas la rendent fausse :
- **`noatime`** : le systeme de fichiers n'enregistre jamais les lectures ; l'atime reste a
  la date de copie. Un modele utilise hier parait dormant depuis des mois → risque de
  proposer quelque chose d'utilise (le mauvais sens).
- **Lecture massive** : un indexeur, une sauvegarde ou un antivirus lit tout le disque un
  jour donne (chez Alexandre : le 2026-09-02) ; tous les atimes tombent ce jour-la. Les
  modeles paraissent recents → rien n'est propose (le sens sur, mais l'outil ment sur
  « dernier usage »).
Le defaut Linux `relatime` est fiable pour une fenetre de 60 jours (l'atime est mis a jour
des qu'il a plus de 24 h).

D1. **Lire les options de montage** (`/proc/mounts`, point de montage le plus long prefixe
    du chemin) pour la racine scannee et pour chaque chemin mesure en atime. `noatime` →
    atime non fiable ; `relatime` / `strictatime` → fiable. Fonction pure, testee sur un
    extrait de `/proc/mounts`.
D2. **Sous `noatime`**, pour les regles mesurees en atime : dernier usage = « inconnu ».
    Recommandation : l'element est classe et affiche (treemap, onglets) mais **non propose**
    (regle R2 : inconnu = pas de proposition), avec la note « last use unknown: filesystem
    mounted noatime ». Un message en tete (app et CLI) donne le total concerne et la cause.
    Variante ecartee par Claude, a confirmer par Alexandre : proposer quand meme en `review`
    avec l'avertissement — plus utile sur les machines `noatime`, mais contraire a R2.
D3. **Detecter la lecture massive** : apres le scan, compter par jour les atimes des gros
    fichiers (hors dossiers de build) ; si un jour autre qu'aujourd'hui ou hier **domine**
    (≥ 25 % d'entre eux, ≥ 2 × le deuxieme jour, ≥ 200 fichiers), c'est une lecture
    massive. [Seuil revu le 2026-10-16 apres mesure reelle : le jour de sauvegarde du
    2026-10-02 portait 35 %, 2,5 × le suivant ; le 40 % initial le manquait.] Pour un
    element dont l'atime tombe ce jour-la, le vrai dernier usage est quelque part entre sa
    date de modification et ce jour : il n'est dormant que si **ce jour-la** a plus de 60
    jours. L'outil le dit : « last read on 2026-09-02, the day most of this disk was read;
    real last use unknown; proposed from 2026-11-01 unless used ». Pas de recuperation
    d'espace immediate, mais une information vraie et une date.
D4. **Affichage** : une ligne sous les KPI (app) et en tete de `recommend` (CLI) quand D2 ou
    D3 s'applique, chiffres a l'appui ; `--explain` liste les elements concernes. Cles i18n.
D5. **Tests** : options de montage et jour de lecture massive injectables dans
    `ClassifyOptions` pour les tests ; detection D3 testee sur un arbre fictif ; D1 testee sur
    une chaine `/proc/mounts`.

- [x] Conception D1–D5 validee par Alexandre, D2 strict (« je suis ta recommandation ») — 2026-10-16
- [x] Code + tests (coeur : module `atime`, 29 tests verts, clippy propre), affichage (app :
      bandeau sous les KPI ; CLI : notes + `--explain`), locales, `docs/rules.md` — commit 15
- [x] `SHA256SUMS` propre : plus de ligne pour lui-meme (constate a la 0.1.1) ; liste
      encore les paquets 0.1.0 restes dans `dist/` → le script ne somme plus que la version
      courante (pour la 0.1.2)
- [x] Ecran de chargement vu par Alexandre sur `/` (664 Go, 3,2 M fichiers) : etape,
      compteurs, pulsation OK ; KPI « 0B » pendant le scan → « — » ; pied de page recalcule
      au changement de langue — 2026-10-16
- [x] Usage reel par Alexandre (2026-10-16) : script `safe` genere, relu par Claude (2
      defauts corriges avant execution), execute : **54,1 Gio liberes** (57,5 annonces,
      2,4 restants) ; arret sur verrou apt → script rendu resilient ; residus < 1 Mio
      non proposes. Suite (13 snaps, apt, /var/crash) a lancer avec le script regenere
- [x] Release **v0.1.1** publiee par Alexandre (2026-10-16) : tag et `main` alignes,
      `.deb` retelecharge et verifie depuis le cloud, binaire 0.1.1 avec la mecanique `run`.
      **Etape 6 close.**
- [ ] Texte de presentation (r/rust, r/linux, Show HN) — a preparer maintenant, sur decision
      d'Alexandre du moment
