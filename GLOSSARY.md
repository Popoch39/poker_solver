# Outil d'étude Expresso Nitro

Outil d'étude hors session pour les Expresso Nitro Winamax : solver préflop, modèle de population tiré des historiques, node-locking, simulateur et, à terme, client local avec bot cliqueur.

## Format de jeu

**Expresso Nitro** :
Spin & Go Winamax à 3 joueurs, 300 jetons de départ, blindes 10/20 (15 BB) et niveaux d'une minute, où le vainqueur prend toute la dotation (sauf aux gros multiplicateurs).
_Éviter_ : Expresso (désigne le format classique à 500 jetons), Spin, Nitro seul

**Multiplicateur** :
Coefficient tiré au hasard avant la partie, appliqué au buy-in pour fixer la dotation.

**Dotation** :
Somme mise en jeu dans une partie, égale au buy-in hors rake multiplié par le multiplicateur.
_Éviter_ : prize pool, cagnotte

**Rake** :
Part du buy-in prélevée par Winamax (7 % de 1 à 5 €, 8 % à 0,25 et 0,50 €).
_Éviter_ : commission, frais

**Seuil de rentabilité** :
Taux de victoire minimal pour que les gains couvrent les buy-ins rake compris (≈ 35,8 % de 1 à 5 €).
_Éviter_ : break-even (en prose), point mort

## Solver

**Spot** :
Situation de décision à résoudre, définie par les joueurs présents, leurs tapis en BB et les actions autorisées.
_Éviter_ : situation, configuration

**Tapis effectif** :
Le plus petit tapis parmi les joueurs engagés dans un coup, mesuré avant de poser les blindes : le maximum qui peut changer de mains.

**Push/fold** :
Arbre d'actions où chaque joueur n'a que deux options : tapis (ou suivre un tapis) ou se coucher.
_Éviter_ : jam/fold, shove/fold

**Limp** :
Entrer dans un pot non relancé en mettant juste la grosse blinde (ou en complétant depuis la petite blinde).
_Éviter_ : call (désigne le fait de payer un tapis ou une relance), complete (en prose)

**Min-raise** :
Relance à deux grosses blindes dans un pot non relancé ; il y en a au plus une par coup dans l'arbre du solver.
_Éviter_ : raise (seul, ambigu avec le tapis), open

**Nœud** :
Point de décision de l'arbre d'actions : un joueur à parler après une suite d'actions donnée (ex. « BB face au tapis de la SB »).
_Éviter_ : spot (désigne la situation entière), état

**Classe de main** :
Une des 169 mains de départ à couleurs près (`AA`, `AKs`, `K7o`…).
_Éviter_ : main (ambigu avec un coup joué), combo (désigne deux cartes précises)

**Range** :
Fréquence de chaque action pour chacune des 169 classes de main à un nœud, présentée en grille 13×13.
_Éviter_ : fourchette, éventail

**Stratégie d'équilibre** :
Ensemble des ranges de tous les nœuds d'un spot dont aucun joueur ne peut s'écarter avec profit ; payoffs en chips, sans ICM.
_Éviter_ : stratégie GTO, Nash (en prose), stratégie optimale

**Exploitabilité** :
Somme, sur les joueurs, de ce que chacun gagnerait par coup (en BB) en passant à sa meilleure réponse pendant que les autres gardent la stratégie évaluée ; nulle à l'équilibre.
_Éviter_ : erreur, distance à Nash

**Facteur de réalisation** :
Fraction de son équité qu'une range récupère réellement quand le coup se joue après le flop, selon la position et le nombre de joueurs.
_Éviter_ : coefficient d'équité, EQR (en prose)

**Modèle d'équité** :
Valorisation d'un coup qui atteint le flop sans joueur à tapis : chaque joueur encore en jeu gagne son facteur de réalisation × son équité × le pot, sans jouer le postflop.
_Éviter_ : postflop (désigne le jeu réel après le flop), modèle postflop

## Population et exploitation

**Historique de mains** :
Fichier texte écrit par le client Winamax qui décrit chaque coup joué (sièges, tapis, cartes connues, actions, gains), accompagné d'un fichier résumé par tournoi.
_Éviter_ : hand history (en prose), log

**Modèle de population** :
Fréquences d'action agrégées par nœud et par tranche de tapis, avec leur taille d'échantillon, tirées des historiques de mains sans tenir compte des pseudos.
_Éviter_ : profil, HUD, stats joueurs

**Tranche de tapis** :
Intervalle de tapis effectif (en BB, borne haute incluse, ex. 8–10 BB) sur lequel le modèle de population regroupe les décisions d'un même nœud.
_Éviter_ : bucket (en prose), palier

**Bot-population** :
Joueur simulé qui agit selon les fréquences du modèle de population.
_Éviter_ : adversaire virtuel, IA

**Node-locking** :
Fixation de certains nœuds sur des fréquences données (celles de la population) avant de re-résoudre le reste du spot, pour obtenir une stratégie d'exploitation.
_Éviter_ : verrouillage (seul), lock

**Stratégie d'exploitation** :
Stratégie du héros obtenue par node-locking : la meilleure réponse aux nœuds verrouillés sur la population, les autres nœuds restant à l'équilibre.
_Éviter_ : stratégie exploitante, exploit (en prose)

**Politique de repli** :
Règle de jeu d'une stratégie push/fold (héros ou bot-population) dans un spot que l'arbre ne couvre pas : postflop, limp, relance qui n'est pas un tapis (voir ADR 0007).
_Éviter_ : fallback (en prose), défaut

## Client local

**Client local** :
Table de poker locale, sous notre contrôle, qui affiche les parties du simulateur ; le seul endroit où un bot a le droit de jouer.
_Éviter_ : client (seul, ambigu avec le client Winamax), table

**Bot cliqueur** :
Programme qui lit l'écran du client local et joue en injectant des clics, sans aucune API.
_Éviter_ : bot (seul), auto-clicker

**Arrêt d'urgence** :
Signal (SIGINT, SIGTERM) ou fichier créé par un raccourci qui coupe aussitôt toute injection de clics du bot cliqueur, sans jamais se lever tout seul (voir ADR 0008).
_Éviter_ : kill switch, pause
