import { Switch, Label, Header } from "@/components";
import { useApp } from "@/contexts";

interface AutostartToggleProps {
  className?: string;
}

export const AutostartToggle = ({ className }: AutostartToggleProps) => {
  const { customizable, toggleAutostart } = useApp();

  const isEnabled = customizable?.autostart?.isEnabled ?? true;

  const handleSwitchChange = async (checked: boolean) => {
    await toggleAutostart(checked);
  };

  return (
    <div className={`space-y-2 ${className}`}>
      <Header
        title="Launch on Startup"
        description="Automatically open Pluely when your system starts"
        isMainTitle
      />
      <div className="flex items-center justify-between">
        <div className="flex items-center space-x-3">
          <div>
            <Label className="text-sm font-medium">Open on Start</Label>
            <p className="text-xs text-muted-foreground mt-1">
              {isEnabled
                ? "Pluely will launch automatically on system startup"
                : "Pluely will not launch automatically"}
            </p>
          </div>
        </div>
        <Switch
          checked={isEnabled}
          onCheckedChange={handleSwitchChange}
          aria-label="Toggle autostart"
        />
      </div>
    </div>
  );
};
