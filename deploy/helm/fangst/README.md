# fangst Helm chart

Runs the Fangst download daemon, `fangst-dl`, as one pod. Releases are published as an OCI chart:

```sh
kubectl create secret generic fangst-api-key --from-literal=api-key="$(openssl rand -hex 24)"
helm install fangst oci://ghcr.io/fangst-media/charts/fangst --version <version> \
  --set apiKey.existingSecret=fangst-api-key \
  --set persistence.downloads.existingClaim=downloads
```

The chart always runs one replica with the `Recreate` strategy: the daemon owns its database and torrent state, and two pods must never open them at once. The daemon does not serve the web app yet; use the API or the qBittorrent and SABnzbd adapters.

## Values

| Value | Default | What it does |
| --- | --- | --- |
| `apiKey.existingSecret`, `apiKey.secretKey` | `""`, `api-key` | Secret holding the first user's API key. |
| `apiKey.value` | `""` | The key itself, at least 16 characters; the chart then creates the Secret. One of the two is required. |
| `timezone` | `Etc/UTC` | `TZ` for the daemon. |
| `env`, `envFrom` | `[]` | Extra environment, for example the secret a `--password-env` flag names. |
| `extraArgs` | `[]` | Extra `fangst-dl serve` flags, such as `--host`, `--connections` or `--enable qbittorrent`. |
| `service.type`, `service.port` | `ClusterIP`, `7740` | Service for the API. |
| `torrent.enabled` | `true` | `false` passes `--no-torrent` and leaves out the torrent Service. |
| `torrent.port` | `6881` | Peer port in the pod, TCP and UDP. |
| `torrent.service.type` | `ClusterIP` | `LoadBalancer`, or `NodePort` with `torrent.service.nodePort`, lets peers connect in. With a node port set, the daemon announces that port. |
| `persistence.data` | 1Gi claim | Database, master key and torrent state at `/data`. `existingClaim` uses your own claim; a claim the chart creates is kept on uninstall. |
| `persistence.downloads` | emptyDir | `existingClaim` or `hostPath` for `/downloads`. An emptyDir loses downloads when the pod is replaced. |
| `persistence.media` | off | `existingClaim` or `hostPath`, mounted at `mountPath` (`/media`). |
| `podSecurityContext`, `securityContext` | uid and gid 1000, read-only root, no capabilities | The pod runs as this user directly; the image's `PUID` and `PGID` do not apply. Folders on `hostPath` must be writable by it. |
| `resources` | 100m CPU and 128Mi requested, 1Gi memory limit | |

Health, readiness and startup probes all use `GET /api/v1/health`, which needs no key.

The daemon reports file paths under `/data/downloads`, a link to `/downloads` made by the image, until the config file can set the download folder (#31). Other pods that import the files need the same volume at `/data/downloads` or a path mapping.
