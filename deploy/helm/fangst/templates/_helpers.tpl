{{- define "fangst.name" -}}
{{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" }}
{{- end }}

{{- define "fangst.fullname" -}}
{{- if .Values.fullnameOverride }}
{{- .Values.fullnameOverride | trunc 63 | trimSuffix "-" }}
{{- else }}
{{- $name := default .Chart.Name .Values.nameOverride }}
{{- if contains $name .Release.Name }}
{{- .Release.Name | trunc 63 | trimSuffix "-" }}
{{- else }}
{{- printf "%s-%s" .Release.Name $name | trunc 63 | trimSuffix "-" }}
{{- end }}
{{- end }}
{{- end }}

{{- define "fangst.selectorLabels" -}}
app.kubernetes.io/name: {{ include "fangst.name" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
{{- end }}

{{- define "fangst.labels" -}}
helm.sh/chart: {{ printf "%s-%s" .Chart.Name .Chart.Version | replace "+" "_" | trunc 63 | trimSuffix "-" }}
{{ include "fangst.selectorLabels" . }}
app.kubernetes.io/version: {{ .Chart.AppVersion | quote }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
{{- end }}

{{- define "fangst.apiKeySecret" -}}
{{- if .Values.apiKey.existingSecret }}
{{- .Values.apiKey.existingSecret }}
{{- else if .Values.apiKey.value }}
{{- include "fangst.fullname" . }}
{{- else }}
{{- fail "set apiKey.existingSecret or apiKey.value" }}
{{- end }}
{{- end }}

{{- define "fangst.dataClaim" -}}
{{- default (printf "%s-data" (include "fangst.fullname" .)) .Values.persistence.data.existingClaim }}
{{- end }}

{{- define "fangst.torrentNodePort" -}}
{{- if and (eq .Values.torrent.service.type "NodePort") .Values.torrent.service.nodePort }}
{{- .Values.torrent.service.nodePort }}
{{- end }}
{{- end }}
