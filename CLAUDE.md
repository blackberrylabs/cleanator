# CLAUDE.md — Cleanator

> Author's working notes, in French. Public documentation lives in `README.md` and `docs/`.

Point d'entree pour reprendre le projet sans contexte. Version du 2026-10-15.
Regles communes a tous mes projets : `../../Projets/00_moi/CONVENTIONS.md`, `AUTONOMIE.md`.
Qui je suis : `../../Projets/00_moi/IDENTITE.md`.

## Ce qu'est ce projet

Nettoyeur de disque pour le developpeur Linux qui fait de l'IA : treemap par categorie
(modeles, caches, Docker, assistants IA, projets dormants), recommandations classees
`safe` / `review` / `never`, et un script shell genere que l'utilisateur lit et lance
lui-meme. L'outil ne supprime jamais rien. 100 % Rust (coeur en crate, CLI, interface
iced). Open source MIT, anglais (francais en option), publie sous
`github.com/blackberrylabs`. Detail dans `BRIEF.md`.

## Etat au 2026-10-15

BRIEF clos (rev. 6 du 2026-10-15 : pile 100 % Rust / iced). Inventaire du disque fait et
analyse (`00_cadrage/`). Etape 1 close : 70 regles v0 en dix fichiers (`rules/`), 10 sources
encore a verifier avant release. Aucun code. Prochaine etape : 2, le coeur Rust en CLI
(espace de travail Cargo, scanner, sondeurs, classifieur, script), sur le « go » d'Alexandre. Detail et suite : `PLAN.md`.

## A lire avant de travailler

`BRIEF.md` (cadrage, surtout §5 regles et §6 tableau), `PLAN.md` (avancement), `JOURNAL.md`
(decisions recentes).

## Commandes

Aucune pour l'instant (pas de code). A remplir a l'etape 2 : installer, lancer le CLI,
lancer l'interface, tester, construire les paquets, publier — chaque commande avec son
chemin absolu, ce qu'elle produit, comment verifier, comment revenir en arriere.

## Regles propres au projet

Toutes dans `BRIEF.md` §5 (R1 a R17). Les trois a ne jamais oublier : R1 jamais de
suppression par l'outil ; R4 un magasin gere se nettoie par la commande de son outil ;
R10 bis metadonnees seulement, fichiers d'identification jamais touches.

## Arborescence

- `00_cadrage/` — references (captures), inventaire du disque.
- `BRIEF.md`, `PLAN.md`, `JOURNAL.md`, `CLAUDE.md` — pilotage, en francais.
- `rules/` — les regles de classement (TOML, anglais), `docs/rules.md` leur schema.
- A venir (etape 2) : `Cargo.toml` racine, `crates/core`, `crates/cli`, `crates/app` (iced),
  `locales/`, `README.md`, `LICENSE`, `CONTRIBUTING.md`.
