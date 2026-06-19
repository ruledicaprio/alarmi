import { Tag } from 'antd'
import { SEV_COLOR, TRANSITION_COLOR, SOURCE_COLOR } from '../colors'
import type { Severity, Transition, Source } from '../api'

export const SeverityTag = ({ v }: { v: Severity }) =>
  <Tag color={SEV_COLOR[v]} style={{ fontWeight: 600, textTransform: 'uppercase', fontSize: 11 }}>{v}</Tag>

export const TransitionTag = ({ v }: { v: Transition }) =>
  <Tag color={TRANSITION_COLOR[v]} style={{ fontSize: 11 }}>{v}</Tag>

export const SourceTag = ({ v }: { v: Source }) =>
  <Tag color={SOURCE_COLOR[v]} style={{ fontSize: 11 }}>{v}</Tag>

export const ClassTag = ({ v }: { v: string }) =>
  <Tag style={{ fontSize: 11 }}>{v}</Tag>
