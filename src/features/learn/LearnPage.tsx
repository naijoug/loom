import { useContext, useEffect, useState } from "react";
import { GraduationCap } from "lucide-react";
import { tauriLearnApi, type LearnStudySummary } from "../../api/learn";
import { hasTauriRuntime } from "../../hooks/runtime";
import { useT } from "../../i18n";
import { useAppState } from "../../state/AppStateContext";
import { LearnLayout, StudySwitcher } from "./LearnLayout";
import { useDiscoverChat } from "./DiscoverChat";
import { NewStudyPage } from "./NewStudyPage";
import { LearnPreviewContext } from "./previewContext";
import { StudyView } from "./StudyView";
import "./learn.css";

export function LearnPage() {
  const t = useT();
  const { state } = useAppState();
  const preview = useContext(LearnPreviewContext);
  const api = preview ?? tauriLearnApi;
  const available = !!preview || hasTauriRuntime();
  const [studies, setStudies] = useState<LearnStudySummary[]>([]);
  const [listError, setListError] = useState("");
  const [selected, setSelected] = useState<string | null>(() => {
    try {
      return localStorage.getItem("loom-learn-study");
    } catch {
      return null;
    }
  });
  const [newStudy, setNewStudy] = useState(false);
  const [revision, setRevision] = useState(0);
  const discover = useDiscoverChat(api);
  useEffect(() => {
    try {
      if (selected) localStorage.setItem("loom-learn-study", selected);
      else localStorage.removeItem("loom-learn-study");
    } catch {
      /* per-device convenience only */
    }
  }, [selected]);
  useEffect(() => {
    if (!available) return;
    let disposed = false;
    api
      .list()
      .then((list) => {
        if (disposed) return;
        setStudies(list.studies);
        setListError(list.storageErrors.join("\n"));
        setSelected((current) =>
          current && list.studies.some((item) => item.studyId === current)
            ? current
            : (list.studies[0]?.studyId ?? null),
        );
      })
      .catch((reason) => {
        if (!disposed) setListError(String(reason));
      });
    return () => {
      disposed = true;
    };
  }, [api, available, revision]);
  useEffect(() => {
    if (!available) return;
    let unlisten: (() => void) | undefined,
      disposed = false;
    void api
      .subscribe(() => setRevision((value) => value + 1))
      .then((stop) => {
        if (disposed) stop();
        else unlisten = stop;
      });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [api, available]);
  const creating = newStudy || selected === null;
  const select = (id: string | null) => {
    setNewStudy(id === null);
    setSelected(id ?? selected);
  };
  const switcher = (
    <StudySwitcher
      studies={studies}
      current={studies.find((item) => item.studyId === selected)}
      creating={creating}
      listError={listError}
      onSelect={select}
    />
  );
  if (available && !creating)
    return (
      <StudyView
        api={api}
        studyId={selected!}
        revision={revision}
        side={switcher}
        onDeleted={() => {
          setSelected(null);
          setRevision((value) => value + 1);
        }}
      />
    );
  return (
    <LearnLayout side={switcher}>
      {!available ? (
        <div className="learn-main-empty">
          <span className="chat-empty-mark">
            <GraduationCap size={24} />
          </span>
          <p>{t("learnDesktopOnly")}</p>
        </div>
      ) : (
        <NewStudyPage
          api={api}
          agents={state.agents}
          diagnostics={state.agentDiagnostics}
          providers={state.providers.providers}
          chat={discover}
          onCreated={(id) => {
            select(id);
            setRevision((value) => value + 1);
          }}
        />
      )}
    </LearnLayout>
  );
}
