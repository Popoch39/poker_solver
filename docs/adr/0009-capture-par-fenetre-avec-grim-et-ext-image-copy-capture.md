---
status: accepted
---

# Capture du client local par fenêtre (`grim -T`, ext-image-copy-capture) plutôt que par wlr-screencopy

Le spec #1 prévoit une capture « via le protocole wlr-screencopy ». Or wlr-screencopy ne capture qu'une sortie (un écran) ou une région de l'écran : une fenêtre posée sur la table, une notification ou un menu, serait capturée avec elle, et le bot lirait des pixels qui ne sont pas ceux du client local. **Décision : le bot capture la seule fenêtre du client local, par `grim -T <identifiant stable>`, qui passe par le protocole ext-image-copy-capture sur la fenêtre (toplevel) du client.** L'image ne contient alors que les pixels du client, même quand une autre fenêtre le recouvre, et rien d'autre du bureau n'est jamais lu.

## Conséquences

- **Seul le client est lu.** C'est la même règle que pour les clics (ADR 0008) : le bot ne regarde que la fenêtre au titre et à l'app_id du client. Si la fenêtre change pendant la capture (le client fermé, son identifiant repris par une autre fenêtre), l'image est jetée.
- **Taille fixe.** L'image n'est lue qu'à la taille du client : la fenêtre doit flotter sans mise à l'échelle, sur un écran à l'échelle 1, entièrement opaque (règles Hyprland documentées dans `nitro_local_client`).
- **Dépendances.** Le bot appelle `grim` en sous-processus (PNG non compressé sur la sortie standard), et le compositeur doit implémenter ext-image-copy-capture et la liste des fenêtres (ext-foreign-toplevel-list). Le trait `Grabber` garde la capture injectable : les tests lisent des images enregistrées, sans écran.
